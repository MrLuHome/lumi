//! 隔离探针：模型验证与冒烟推理放进独立子进程，带硬超时与崩溃隔离。
//!
//! 子进程是应用自身的 `--face-model-probe` 模式；不执行模型内的任何脚本，
//! 也不把模型路径拼进 shell。超时后直接 kill，主进程不会被人为构造的
//! 死循环模型拖死。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tract_onnx::prelude::*;

use super::security::read_limited;
use super::validator::{validate_onnx, OnnxProbe};

pub const PROBE_FLAG: &str = "--face-model-probe";
pub const INFER_FLAG: &str = "--face-model-infer";
pub const DEFAULT_TIMEOUT_MS: u64 = 20_000;
pub const DEFAULT_INFER_TIMEOUT_MS: u64 = 10_000;

/// 单次推理 Worker 的硬限制。超限直接判失败，不尝试“跑完再说”。
pub const MAX_INFER_EDGE: u32 = 1024;
pub const MAX_INFER_ELEMENTS: usize = 3 * 1024 * 1024;
pub const INFER_MEMORY_LIMIT_BYTES: u64 = 1_536 * 1024 * 1024;
pub const INFER_CPU_LIMIT_SECONDS: u64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferRequest {
    pub model_path: String,
    pub input_path: String,
    pub width: u32,
    pub height: u32,
    pub expected_points: usize,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferResponse {
    pub ok: bool,
    pub points: Vec<[f32; 3]>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

/// Worker 内执行：读取预处理张量，加载模型并推理。
/// 所有失败都转成 `error`，不 panic、不退出进程异常码。
pub fn infer_sync(request: &InferRequest) -> InferResponse {
    let started = Instant::now();
    let fail = |message: String| InferResponse {
        ok: false,
        points: Vec::new(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        error: Some(message),
    };
    if !(32..=MAX_INFER_EDGE).contains(&request.width)
        || !(32..=MAX_INFER_EDGE).contains(&request.height)
    {
        return fail(format!(
            "输入尺寸 {}×{} 超出 32..1024",
            request.width, request.height
        ));
    }
    let elements = 3usize * request.width as usize * request.height as usize;
    if elements > MAX_INFER_ELEMENTS {
        return fail(format!(
            "输入张量元素数 {elements} 超过上限 {MAX_INFER_ELEMENTS}"
        ));
    }
    if request.expected_points != 68 {
        return fail(format!(
            "期望点数必须是 68，实际 {}",
            request.expected_points
        ));
    }
    let bytes = match read_limited(Path::new(&request.input_path), 64 * 1024 * 1024) {
        Ok(bytes) => bytes,
        Err(error) => return fail(format!("读取输入张量失败：{error}")),
    };
    if bytes.len() != elements * 4 {
        return fail(format!(
            "输入文件大小 {} 与 {}×4 不符",
            bytes.len(),
            elements
        ));
    }
    let mut data = Vec::with_capacity(elements);
    for chunk in bytes.chunks_exact(4) {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if !value.is_finite() {
            return fail("输入张量包含 NaN/Inf".into());
        }
        data.push(value);
    }
    let installed_dir = match Path::new(&request.model_path).parent() {
        Some(parent) => parent.to_path_buf(),
        None => return fail("模型路径没有父目录".into()),
    };
    let manifest_path = installed_dir.join("manifest.json");
    let manifest = match std::fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<super::manifest::ModelManifest>(&bytes).ok())
    {
        Some(manifest) => manifest,
        None => return fail("读取模型 manifest 失败".into()),
    };
    match super::inference::run_plan(
        Path::new(&request.model_path),
        &manifest,
        data,
        request.width as usize,
        request.height as usize,
    ) {
        Ok(points) => InferResponse {
            ok: true,
            points: points
                .into_iter()
                .map(|(x, y, confidence)| [x, y, confidence])
                .collect(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: None,
        },
        Err(error) => fail(error),
    }
}

#[cfg(unix)]
fn apply_worker_limits(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            let memory = libc::rlimit {
                rlim_cur: INFER_MEMORY_LIMIT_BYTES,
                rlim_max: INFER_MEMORY_LIMIT_BYTES,
            };
            let _ = libc::setrlimit(libc::RLIMIT_AS, &memory);
            let cpu = libc::rlimit {
                rlim_cur: INFER_CPU_LIMIT_SECONDS,
                rlim_max: INFER_CPU_LIMIT_SECONDS,
            };
            let _ = libc::setrlimit(libc::RLIMIT_CPU, &cpu);
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn apply_worker_limits(_command: &mut Command) {}

/// 在独立 Worker 进程执行真实推理。崩溃、超时、无响应都返回可读错误，
/// 主进程只负责读写临时文件。
pub fn infer_isolated(
    executable: &Path,
    request: &InferRequest,
    timeout: Duration,
) -> Result<InferResponse, String> {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建推理目录失败：{error}"))?;
    let request_path = dir.join("infer-request.json");
    let response_path = dir.join("infer-response.json");
    let json =
        serde_json::to_string(request).map_err(|error| format!("序列化请求失败：{error}"))?;
    std::fs::write(&request_path, json).map_err(|error| format!("写入推理请求失败：{error}"))?;
    let mut command = Command::new(executable);
    command
        .arg(INFER_FLAG)
        .arg(&request_path)
        .arg(&response_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_worker_limits(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动推理 Worker：{error}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_dir_all(&dir);
                    return Err(format!(
                        "模型推理超时（{} 秒）；已终止 Worker",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = std::fs::remove_dir_all(&dir);
                return Err(format!("等待推理 Worker 失败：{error}"));
            }
        }
    }
    let bytes = std::fs::read(&response_path).map_err(|error| {
        let _ = std::fs::remove_dir_all(&dir);
        format!("推理 Worker 未产生结果（可能崩溃）：{error}")
    })?;
    let _ = std::fs::remove_dir_all(&dir);
    serde_json::from_slice(&bytes).map_err(|error| format!("推理结果解析失败：{error}"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeResult {
    pub attempted: bool,
    pub ok: bool,
    pub output_elements: usize,
    pub finite_ratio: f32,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReport {
    pub probe: OnnxProbe,
    pub smoke: SmokeResult,
}

/// 在零输入上跑一次，确认模型不是“能编译但一跑就失败”。
pub fn smoke_run(path: &Path, probe: &OnnxProbe) -> SmokeResult {
    let started = Instant::now();
    if !probe.ok {
        return SmokeResult {
            attempted: false,
            ok: false,
            output_elements: 0,
            finite_ratio: 0.0,
            elapsed_ms: 0,
            error: Some("结构校验未通过，跳过冒烟推理".into()),
        };
    }
    let (Some(width), Some(height)) = (probe.input_width, probe.input_height) else {
        return SmokeResult {
            attempted: false,
            ok: false,
            output_elements: 0,
            finite_ratio: 0.0,
            elapsed_ms: 0,
            error: Some("输入尺寸未知".into()),
        };
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(usize, f32), String> {
            let model = tract_onnx::onnx()
                .model_for_path(path)
                .map_err(|error| format!("加载失败：{error}"))?
                .into_optimized()
                .map_err(|error| format!("优化失败：{error}"))?
                .into_runnable()
                .map_err(|error| format!("编译失败：{error}"))?;
            let input =
                tract_ndarray::Array4::<f32>::zeros((1, 3, height as usize, width as usize));
            let outputs = model
                .run(tvec!(input.into_tensor().into_tvalue()))
                .map_err(|error| format!("推理失败：{error}"))?;
            let output = outputs.first().ok_or_else(|| "没有输出".to_string())?;
            let view = output
                .to_plain_array_view::<f32>()
                .map_err(|error| format!("输出解析失败：{error}"))?;
            let total = view.len();
            let finite = view.iter().filter(|value| value.is_finite()).count();
            let ratio = if total == 0 {
                0.0
            } else {
                finite as f32 / total as f32
            };
            Ok((total, ratio))
        },
    ));
    match result {
        Ok(Ok((elements, ratio))) => SmokeResult {
            attempted: true,
            ok: elements > 0 && ratio > 0.99,
            output_elements: elements,
            finite_ratio: ratio,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: if ratio <= 0.99 {
                Some("输出包含 NaN/Inf".into())
            } else {
                None
            },
        },
        Ok(Err(error)) => SmokeResult {
            attempted: true,
            ok: false,
            output_elements: 0,
            finite_ratio: 0.0,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some(error),
        },
        Err(_) => SmokeResult {
            attempted: true,
            ok: false,
            output_elements: 0,
            finite_ratio: 0.0,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: Some("冒烟推理 panic，已隔离".into()),
        },
    }
}

pub fn probe_sync(path: &Path) -> ProbeReport {
    let probe = validate_onnx(path);
    let smoke = smoke_run(path, &probe);
    ProbeReport { probe, smoke }
}

fn temp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("lumi-probe-{}-{nanos}", std::process::id()))
}

/// 启动隔离子进程。失败/超时/子进程崩溃都返回可读错误，不 panic。
pub fn probe_isolated(
    executable: &Path,
    model: &Path,
    timeout: Duration,
) -> Result<ProbeReport, String> {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建探针目录失败：{error}"))?;
    let request = dir.join("request.json");
    let response = dir.join("response.json");
    let request_json = serde_json::json!({ "modelPath": model.to_string_lossy() });
    std::fs::write(&request, request_json.to_string())
        .map_err(|error| format!("写入探针请求失败：{error}"))?;
    let mut child = Command::new(executable)
        .arg(PROBE_FLAG)
        .arg(&request)
        .arg(&response)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("无法启动模型探针进程：{error}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_dir_all(&dir);
                    return Err(format!(
                        "模型验证超时（{} 秒）；已强制终止隔离进程",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = std::fs::remove_dir_all(&dir);
                return Err(format!("等待模型探针失败：{error}"));
            }
        }
    }
    let bytes = std::fs::read(&response).map_err(|error| {
        let _ = std::fs::remove_dir_all(&dir);
        format!("模型探针未产生结果（可能崩溃）：{error}")
    })?;
    let _ = std::fs::remove_dir_all(&dir);
    let report: ProbeReport =
        serde_json::from_slice(&bytes).map_err(|error| format!("模型探针结果解析失败：{error}"))?;
    Ok(report)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeRequest {
    model_path: String,
}

/// 子进程入口。返回进程退出码。
pub fn run_cli(args: &[String]) -> i32 {
    if args.len() < 4 {
        eprintln!("用法：{PROBE_FLAG}|{INFER_FLAG} <request.json> <response.json>");
        return 2;
    }
    let request_path = Path::new(&args[2]);
    let response_path = Path::new(&args[3]);
    if args[1] == INFER_FLAG {
        let request = match read_limited(request_path, 64 * 1024).and_then(|bytes| {
            serde_json::from_slice::<InferRequest>(&bytes).map_err(|error| error.to_string())
        }) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("推理请求无效：{error}");
                return 3;
            }
        };
        let response = infer_sync(&request);
        let json = match serde_json::to_vec(&response) {
            Ok(json) => json,
            Err(error) => {
                eprintln!("推理结果序列化失败：{error}");
                return 4;
            }
        };
        if let Err(error) = std::fs::write(response_path, json) {
            eprintln!("推理结果写入失败：{error}");
            return 5;
        }
        return 0;
    }
    let request = match read_limited(request_path, 64 * 1024).and_then(|bytes| {
        serde_json::from_slice::<ProbeRequest>(&bytes).map_err(|error| error.to_string())
    }) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("探针请求无效：{error}");
            return 3;
        }
    };
    let report = probe_sync(Path::new(&request.model_path));
    let json = match serde_json::to_vec(&report) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("探针结果序列化失败：{error}");
            return 4;
        }
    };
    if let Err(error) = std::fs::write(response_path, json) {
        eprintln!("探针结果写入失败：{error}");
        return 5;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_model_smoke_is_not_attempted() {
        let probe = OnnxProbe {
            ok: false,
            file_size: 0,
            sha256: String::new(),
            ir_version: None,
            opset: None,
            input_shape: Vec::new(),
            output_shape: Vec::new(),
            input_width: None,
            input_height: None,
            output_layout: None,
            point_count: None,
            operator_count: 0,
            warnings: Vec::new(),
            errors: vec!["bad".into()],
            elapsed_ms: 0,
        };
        let smoke = smoke_run(Path::new("/tmp/none.onnx"), &probe);
        assert!(!smoke.attempted && !smoke.ok);
    }

    #[test]
    fn cli_without_arguments_returns_usage() {
        assert_eq!(run_cli(&[PROBE_FLAG.to_string()]), 2);
    }

    fn infer_request(width: u32, height: u32, expected_points: usize) -> InferRequest {
        InferRequest {
            model_path: "/tmp/none/model.onnx".into(),
            input_path: "/tmp/none/input.f32".into(),
            width,
            height,
            expected_points,
            timeout_ms: DEFAULT_INFER_TIMEOUT_MS,
        }
    }

    #[test]
    fn infer_rejects_oversized_input_before_reading() {
        let response = infer_sync(&infer_request(2048, 2048, 68));
        assert!(!response.ok);
        assert!(response.error.unwrap().contains("32..1024"));
    }

    #[test]
    fn infer_rejects_wrong_point_contract() {
        let response = infer_sync(&infer_request(64, 64, 5));
        assert!(!response.ok);
        assert!(response.error.unwrap().contains("68"));
    }

    #[test]
    fn infer_rejects_missing_input_file() {
        let response = infer_sync(&infer_request(64, 64, 68));
        assert!(!response.ok);
        assert!(response.error.unwrap().contains("读取输入张量失败"));
    }
}
