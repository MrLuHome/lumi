//! GPU 状态与基准命令。

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::gpu::{BackendPreference, BenchRow, ComputeEngine, GpuStatus};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuBenchmarkResult {
    pub rows: Vec<BenchRow>,
    pub verified_speedup: Option<f64>,
    /// 是否达到默认启用的 1.25× 门槛。
    pub auto_enabled: bool,
    pub status: GpuStatus,
}

#[tauri::command]
pub fn gpu_status(engine: State<'_, ComputeEngine>) -> GpuStatus {
    engine.status()
}

#[tauri::command]
pub async fn gpu_benchmark(engine: State<'_, ComputeEngine>) -> AppResult<GpuBenchmarkResult> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rows = engine.run_benchmark().map_err(AppError::Unsupported)?;
        let verified = engine.verified_speedup();
        Ok(GpuBenchmarkResult {
            rows,
            verified_speedup: verified,
            auto_enabled: verified.map(|value| value >= 1.25).unwrap_or(false),
            status: engine.status(),
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[tauri::command]
pub fn set_compute_backend(
    engine: State<'_, ComputeEngine>,
    preference: BackendPreference,
) -> GpuStatus {
    engine.set_preference(preference);
    engine.status()
}
