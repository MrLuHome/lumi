//! GPU 加速门面：设备探测、后端选择、CPU 回退与基准。
//!
//! 默认策略：
//! - `Auto`：仅当本机基准实测端到端 ≥1.25× 时才启用 GPU，否则 CPU；
//! - `Gpu`：初始化成功即启用（用户显式选择）；
//! - `Cpu`：强制 CPU。
//!
//! 任何失败都会把原因记录在 `GpuStatus.reason` 中，并在状态栏显示。

pub mod device;
pub mod ops;

use std::sync::{Arc, OnceLock, RwLock};

use serde::{Deserialize, Serialize};

pub use device::{BenchRow, GpuCapabilities, GpuEngine};
pub use ops::RgbaF32Image;

/// 计算后端偏好。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BackendPreference {
    #[default]
    Auto,
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    pub preference: BackendPreference,
    pub available: bool,
    pub enabled: bool,
    /// `metal` / `vulkan` / `cpu`。
    pub backend: String,
    pub device: String,
    pub reason: String,
    pub verified_speedup: Option<f64>,
    pub capabilities: Option<GpuCapabilities>,
}

#[derive(Default, Clone)]
pub struct ComputeEngine {
    preference: Arc<RwLock<BackendPreference>>,
    gpu: Arc<OnceLock<Result<GpuEngine, String>>>,
    verified_speedup: Arc<RwLock<Option<f64>>>,
    /// 仅测试使用：注入初始化失败，验证 CPU 回退路径。
    #[cfg(test)]
    forced_failure: Arc<OnceLock<String>>,
}

impl ComputeEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn preference(&self) -> BackendPreference {
        self.preference
            .read()
            .map(|guard| *guard)
            .unwrap_or_default()
    }

    pub fn set_preference(&self, preference: BackendPreference) {
        if let Ok(mut guard) = self.preference.write() {
            *guard = preference;
        }
    }

    /// 惰性初始化 GPU；失败原因会被缓存并返回。
    pub fn gpu(&self) -> Result<&GpuEngine, String> {
        #[cfg(test)]
        if let Some(message) = self.forced_failure.get() {
            return Err(message.clone());
        }
        match self.gpu.get_or_init(GpuEngine::new) {
            Ok(engine) => Ok(engine),
            Err(error) => Err(error.clone()),
        }
    }

    /// 仅测试使用：注入 GPU 初始化失败。
    #[cfg(test)]
    pub fn inject_failure(&self, message: &str) {
        let _ = self.forced_failure.set(message.to_string());
    }

    pub fn verified_speedup(&self) -> Option<f64> {
        self.verified_speedup.read().ok().and_then(|guard| *guard)
    }

    /// 运行基准并把 12MP 档位（或最大档）的端到端倍率记录为启用依据。
    pub fn run_benchmark(&self) -> Result<Vec<BenchRow>, String> {
        let engine = self.gpu()?;
        let sizes = [(1600u32, 1200u32), (3000, 4000), (6000, 4000)];
        let mut rows = Vec::with_capacity(sizes.len());
        for (width, height) in sizes {
            rows.push(engine.benchmark(width, height)?);
        }
        let criterion = rows
            .iter()
            .find(|row| row.width == 3000)
            .or_else(|| {
                rows.iter().max_by(|a, b| {
                    a.speedup
                        .partial_cmp(&b.speedup)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })
            .map(|row| row.speedup);
        if let Ok(mut guard) = self.verified_speedup.write() {
            *guard = criterion;
        }
        Ok(rows)
    }

    pub fn status(&self) -> GpuStatus {
        let preference = self.preference();
        match self.gpu() {
            Ok(engine) => {
                let capabilities = engine.capabilities().clone();
                let speedup = self.verified_speedup();
                let (enabled, reason) = match preference {
                    BackendPreference::Cpu => (false, "用户选择 CPU 模式".to_string()),
                    BackendPreference::Gpu => (true, "用户显式启用 GPU".to_string()),
                    BackendPreference::Auto => match speedup {
                        Some(value) if value >= 1.25 => {
                            (true, format!("自动启用：端到端 {value:.2}×"))
                        }
                        Some(value) => (
                            false,
                            format!("未达 1.25× 门槛（实测 {value:.2}×），使用 CPU"),
                        ),
                        None => (false, "尚未完成基准验证，暂用 CPU".to_string()),
                    },
                };
                GpuStatus {
                    preference,
                    available: true,
                    enabled,
                    backend: if enabled {
                        capabilities.backend.to_lowercase()
                    } else {
                        "cpu".to_string()
                    },
                    device: capabilities.device_name.clone(),
                    reason,
                    verified_speedup: speedup,
                    capabilities: Some(capabilities),
                }
            }
            Err(error) => GpuStatus {
                preference,
                available: false,
                enabled: false,
                backend: "cpu".to_string(),
                device: "CPU".to_string(),
                reason: format!("GPU 不可用，已回退 CPU：{error}"),
                verified_speedup: None,
                capabilities: None,
            },
        }
    }
}

/// 供管线调用的执行器：按状态决定走 GPU 还是 CPU。
pub fn exposure_with_fallback(
    engine: &ComputeEngine,
    image: &RgbaF32Image,
    exposure_ev: f32,
    matrix: Option<[[f32; 3]; 3]>,
) -> (RgbaF32Image, bool) {
    if engine.status().enabled {
        if let Ok(gpu) = engine.gpu() {
            if let Ok(out) = gpu.apply_exposure(image, exposure_ev, matrix) {
                return (out, true);
            }
        }
    }
    (ops::cpu_exposure(image, exposure_ev, matrix), false)
}

pub fn curve_with_fallback(
    engine: &ComputeEngine,
    image: &RgbaF32Image,
    lut: &[f32; 256],
    saturation: f32,
) -> (RgbaF32Image, bool) {
    if engine.status().enabled {
        if let Ok(gpu) = engine.gpu() {
            if let Ok(out) = gpu.apply_curve(image, lut, saturation) {
                return (out, true);
            }
        }
    }
    (ops::cpu_curve(image, lut, saturation), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_management::lab::{delta_e00, rgb16_to_lab};

    fn gradient(width: u32, height: u32) -> RgbaF32Image {
        let mut image = RgbaF32Image::new(width, height);
        for (index, pixel) in image.data.iter_mut().enumerate() {
            let x = (index as u32 % width) as f32 / (width.max(2) - 1) as f32;
            let y = (index as u32 / width) as f32 / (height.max(2) - 1) as f32;
            *pixel = [x, y, (x * y).sqrt(), 1.0];
        }
        image
    }

    #[test]
    fn gpu_exposure_matches_cpu_when_available() {
        let engine = match GpuEngine::new() {
            Ok(engine) => engine,
            Err(error) => {
                eprintln!("[跳过] GPU 不可用：{error}");
                return;
            }
        };
        println!("GPU: {:?}", engine.capabilities());
        let image = gradient(64, 48);
        let cpu = ops::cpu_exposure(&image, 0.7, None);
        let gpu = engine.apply_exposure(&image, 0.7, None).unwrap();
        assert_eq!(cpu.data.len(), gpu.data.len());
        let mut deltas = Vec::with_capacity(cpu.data.len());
        for (left, right) in cpu.data.iter().zip(gpu.data.iter()) {
            assert!(right[0].is_finite() && right[1].is_finite() && right[2].is_finite());
            assert!((right[3] - 1.0).abs() < 1e-6, "Alpha 必须保留");
            let l = rgb16_to_lab([
                (left[0] * 65535.0) as u16,
                (left[1] * 65535.0) as u16,
                (left[2] * 65535.0) as u16,
            ]);
            let r = rgb16_to_lab([
                (right[0] * 65535.0) as u16,
                (right[1] * 65535.0) as u16,
                (right[2] * 65535.0) as u16,
            ]);
            deltas.push(delta_e00(l, r));
        }
        deltas.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean: f32 = deltas.iter().sum::<f32>() / deltas.len() as f32;
        let p95 = deltas[(deltas.len() as f32 * 0.95) as usize];
        println!("曝光一致性：mean ΔE00={mean:.4} p95={p95:.4}");
        assert!(mean <= 0.5, "平均 ΔE00 {mean} 超过 0.5");
        assert!(p95 <= 1.0, "P95 ΔE00 {p95} 超过 1.0");
    }

    #[test]
    fn gpu_curve_matches_cpu_when_available() {
        let engine = match GpuEngine::new() {
            Ok(engine) => engine,
            Err(error) => {
                eprintln!("[跳过] GPU 不可用：{error}");
                return;
            }
        };
        let image = gradient(40, 40);
        let lut = ops::identity_lut();
        let cpu = ops::cpu_curve(&image, &lut, 0.25);
        let gpu = engine.apply_curve(&image, &lut, 0.25).unwrap();
        for (left, right) in cpu.data.iter().zip(gpu.data.iter()) {
            for channel in 0..3 {
                assert!(
                    (left[channel] - right[channel]).abs() < 0.01,
                    "曲线不一致：{left:?} vs {right:?}"
                );
            }
        }
    }

    #[test]
    fn preference_switch_reports_reason() {
        let engine = ComputeEngine::new();
        engine.set_preference(BackendPreference::Cpu);
        let status = engine.status();
        assert!(!status.enabled);
        assert!(status.reason.contains("CPU"));
        engine.set_preference(BackendPreference::Auto);
        let status = engine.status();
        if status.available {
            assert!(!status.reason.is_empty());
        }
    }

    #[test]
    fn injected_gpu_failure_falls_back_to_cpu() {
        let engine = ComputeEngine::new();
        engine.set_preference(BackendPreference::Gpu);
        engine.inject_failure("注入的测试故障");
        let image = gradient(8, 8);

        let (exposed, used_gpu) = exposure_with_fallback(&engine, &image, 1.0, None);
        assert!(!used_gpu, "注入失败后不得声称使用了 GPU");
        assert_eq!(exposed.data, ops::cpu_exposure(&image, 1.0, None).data);

        let lut = ops::identity_lut();
        let (curved, used_gpu) = curve_with_fallback(&engine, &image, &lut, 0.2);
        assert!(!used_gpu);
        assert_eq!(curved.data, ops::cpu_curve(&image, &lut, 0.2).data);

        let status = engine.status();
        assert!(!status.available);
        assert!(!status.enabled);
        assert!(status.reason.contains("注入的测试故障"));
    }

    /// 端到端基准：`cargo test --lib gpu_benchmark_report -- --ignored --nocapture`
    #[test]
    #[ignore = "需要真实 GPU，手动运行"]
    fn gpu_benchmark_report() {
        let engine = ComputeEngine::new();
        match engine.run_benchmark() {
            Ok(rows) => {
                println!(
                    "{:>5}×{:<5} {:>8} {:>10} {:>10} {:>10} {:>10} {:>9}",
                    "W", "H", "MP", "CPU ms", "上传 ms", "GPU 运算", "GPU 总", "倍率"
                );
                for row in rows {
                    println!(
                        "{:>5}×{:<5} {:>8.1} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>8.2}×",
                        row.width,
                        row.height,
                        row.megapixels,
                        row.cpu_ms,
                        row.gpu_upload_ms,
                        row.gpu_compute_ms,
                        row.gpu_total_ms,
                        row.speedup
                    );
                }
                println!("Auto 启用依据：{:?}", engine.verified_speedup());
                let status = engine.status();
                println!("状态：{} / {}", status.backend, status.reason);
            }
            Err(error) => println!("[跳过] GPU 基准不可用：{error}"),
        }
    }
}
