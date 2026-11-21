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
pub const DEFAULT_TIMEOUT_MS: u64 = 20_000;

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
        eprintln!("用法：{PROBE_FLAG} <request.json> <response.json>");
        return 2;
    }
    let request_path = Path::new(&args[2]);
    let response_path = Path::new(&args[3]);
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
}
