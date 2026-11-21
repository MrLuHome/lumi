use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::ai_engine::face_frame::FaceFrame;
use crate::error::{AppError, AppResult};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::export::{self, ExportFormat, ExportSettings};
use crate::image_engine::params::EditParams;
use crate::image_engine::pipeline;
use crate::state::{FaceRepository, ImageCache};

pub const BATCH_EVENT: &str = "lumi://batch-progress";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BatchStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    #[default]
    Skip,
    Rename,
    Overwrite,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchTask {
    pub job_id: String,
    pub path: String,
    pub name: String,
    /// 本批次内的输出编号（起点由用户设置，默认 1）。
    pub number: u32,
    pub status: BatchStatus,
    pub stage: String,
    pub progress: f32,
    pub output_path: Option<String>,
    pub error: Option<String>,
    /// 0 表示尚未开始。
    pub started_at_ms: u64,
    /// 0 表示尚未结束。
    pub finished_at_ms: u64,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    /// 每张照片自己的编辑参数，不参与序列化，批量之间互不合并。
    #[serde(skip)]
    params: EditParams,
    /// RAW 显影参数（仅 RAW 文件使用）。
    #[serde(skip)]
    raw_params: Option<crate::image_engine::raw::RawDevelopParams>,
}

impl BatchTask {
    fn reset(&mut self) {
        self.status = BatchStatus::Pending;
        self.stage = String::new();
        self.progress = 0.0;
        self.output_path = None;
        self.error = None;
        self.started_at_ms = 0;
        self.finished_at_ms = 0;
        self.bytes = 0;
        self.width = 0;
        self.height = 0;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCurrent {
    pub job_id: String,
    pub name: String,
    pub index: usize,
    pub stage: String,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSnapshot {
    pub tasks: Vec<BatchTask>,
    pub running: bool,
    pub paused: bool,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: usize,
    pub pending: usize,
    pub exported: usize,
    pub current: Option<BatchCurrent>,
    pub overall: f32,
    pub elapsed_ms: u64,
    /// 只能按已完成任务的实际耗时估算；样本不足时为 null。
    pub eta_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPhoto {
    pub path: String,
    pub params: EditParams,
    /// RAW 文件的显影参数；非 RAW 时忽略。
    #[serde(default)]
    pub raw_params: Option<crate::image_engine::raw::RawDevelopParams>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchExportRequest {
    pub photos: Vec<BatchPhoto>,
    pub settings: ExportSettings,
    pub output_dir: String,
    /// 支持 {name} / {index} / {format}，默认 {name}-lumi。
    #[serde(default = "default_naming")]
    pub naming: String,
    #[serde(default = "default_index")]
    pub start_index: u32,
    #[serde(default = "default_conflict")]
    pub conflict: ConflictPolicy,
    /// 覆盖已有文件必须由用户明确确认。
    #[serde(default)]
    pub overwrite_confirmed: bool,
}

fn default_naming() -> String {
    "{name}-lumi".into()
}

fn default_index() -> u32 {
    1
}

fn default_conflict() -> ConflictPolicy {
    ConflictPolicy::Skip
}

#[derive(Default)]
struct BatchState {
    tasks: Vec<BatchTask>,
    running: bool,
    settings: ExportSettings,
    output_dir: PathBuf,
    naming: String,
    start_index: u32,
    conflict: ConflictPolicy,
    overwrite_confirmed: bool,
    started_at_ms: u64,
    finished_at_ms: u64,
    sequence: u64,
    /// 导出时的输出 Profile；批量与单张共用同一套色彩设置。
    color: crate::color_management::ColorOutputSettings,
}

/// 批量导出引擎。后台只有一个工作线程，逐张完整渲染、写盘后再处理下一张，
/// 内存峰值可控，单张失败不会影响其余任务。
#[derive(Clone)]
pub struct BatchEngine {
    inner: Arc<Mutex<BatchState>>,
    pause: Arc<AtomicBool>,
    cancel_all: Arc<AtomicBool>,
    cancel_current: Arc<AtomicBool>,
    jobs: Arc<AtomicU64>,
}

impl Default for BatchEngine {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BatchState::default())),
            pause: Arc::new(AtomicBool::new(false)),
            cancel_all: Arc::new(AtomicBool::new(false)),
            cancel_current: Arc::new(AtomicBool::new(false)),
            jobs: Arc::new(AtomicU64::new(1)),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn lock_error() -> AppError {
    AppError::Task("批处理状态异常，请重试".into())
}

fn snapshot_of(state: &BatchState, paused: bool, now: u64) -> BatchSnapshot {
    let total = state.tasks.len();
    let mut completed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut cancelled = 0;
    let mut pending = 0;
    let mut exported = 0;
    let mut current = None;
    let mut progress_sum = 0.0f32;
    let mut sample_total = 0u64;
    let mut sample_count = 0u64;

    for (index, task) in state.tasks.iter().enumerate() {
        let done = matches!(
            task.status,
            BatchStatus::Completed | BatchStatus::Failed | BatchStatus::Cancelled | BatchStatus::Skipped
        );
        progress_sum += if done { 1.0 } else { task.progress.clamp(0.0, 1.0) };
        match task.status {
            BatchStatus::Completed => {
                completed += 1;
                exported += 1;
            }
            BatchStatus::Failed => failed += 1,
            BatchStatus::Skipped => skipped += 1,
            BatchStatus::Cancelled => cancelled += 1,
            BatchStatus::Pending => pending += 1,
            BatchStatus::Running => {
                current = Some(BatchCurrent {
                    job_id: task.job_id.clone(),
                    name: task.name.clone(),
                    index,
                    stage: task.stage.clone(),
                    progress: task.progress,
                });
            }
        }
        if done && task.started_at_ms > 0 && task.finished_at_ms >= task.started_at_ms {
            sample_total += task.finished_at_ms - task.started_at_ms;
            sample_count += 1;
        }
    }

    let remaining = total as u64 - (completed + failed + skipped + cancelled) as u64;
    let eta_ms = if state.running && sample_count > 0 && remaining > 0 {
        Some(sample_total / sample_count * remaining)
    } else {
        None
    };

    BatchSnapshot {
        tasks: state.tasks.clone(),
        running: state.running,
        paused,
        total,
        completed,
        failed,
        skipped,
        cancelled,
        pending,
        exported,
        current,
        overall: if total == 0 { 0.0 } else { progress_sum / total as f32 },
        elapsed_ms: if state.started_at_ms == 0 {
            0
        } else if state.running {
            now.saturating_sub(state.started_at_ms)
        } else {
            state.finished_at_ms.saturating_sub(state.started_at_ms)
        },
        eta_ms,
    }
}

impl BatchEngine {
    pub fn snapshot(&self) -> BatchSnapshot {
        let paused = self.pause.load(Ordering::Relaxed);
        match self.inner.lock() {
            Ok(state) => snapshot_of(&state, paused, now_ms()),
            Err(_) => snapshot_of(&BatchState::default(), paused, now_ms()),
        }
    }

    /// 更新批量导出使用的输出 Profile（与单张导出共用设置）。
    pub fn set_color(&self, color: crate::color_management::ColorOutputSettings) {
        if let Ok(mut state) = self.inner.lock() {
            state.color = color;
        }
    }

    /// 窗口对象在测试中不可构造，因此允许传入 None：队列本身照常运行，
    /// 只是不会向前端推送进度事件。
    fn emit(&self, app: Option<&AppHandle>) {
        if let Some(handle) = app {
            let _ = handle.emit(BATCH_EVENT, self.snapshot());
        }
    }

    fn begin(&self, request: BatchExportRequest) -> AppResult<BatchSnapshot> {
        if request.photos.is_empty() {
            return Err(AppError::InvalidInput("请先选择要导出的照片".into()));
        }
        let dir = request.output_dir.trim();
        if dir.is_empty() {
            return Err(AppError::InvalidInput("请选择输出目录".into()));
        }
        let output_dir = PathBuf::from(dir);
        std::fs::create_dir_all(&output_dir)?;
        if !output_dir.is_dir() {
            return Err(AppError::InvalidInput("输出目录不可用".into()));
        }

        let stamp = now_ms();
        let mut tasks = Vec::with_capacity(request.photos.len());
        for (index, photo) in request.photos.into_iter().enumerate() {
            let name = Path::new(&photo.path)
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("照片 {}", index + 1));
            let sequence = self.jobs.fetch_add(1, Ordering::Relaxed);
            tasks.push(BatchTask {
                job_id: format!("job-{stamp}-{sequence}"),
                path: photo.path,
                name,
                number: request.start_index.max(1) + index as u32,
                status: BatchStatus::Pending,
                stage: String::new(),
                progress: 0.0,
                output_path: None,
                error: None,
                started_at_ms: 0,
                finished_at_ms: 0,
                bytes: 0,
                width: 0,
                height: 0,
                params: photo.params,
                raw_params: photo.raw_params,
            });
        }

        let mut state = self.inner.lock().map_err(|_| lock_error())?;
        if state.running {
            return Err(AppError::InvalidInput("已有批处理任务正在运行".into()));
        }
        state.tasks = tasks;
        state.running = true;
        state.settings = request.settings.clamped();
        state.output_dir = output_dir;
        state.naming = if request.naming.trim().is_empty() {
            default_naming()
        } else {
            request.naming.trim().to_string()
        };
        state.start_index = request.start_index.max(1);
        state.conflict = request.conflict;
        state.overwrite_confirmed = request.overwrite_confirmed;
        state.started_at_ms = stamp;
        state.finished_at_ms = 0;
        state.sequence = 0;
        drop(state);

        self.pause.store(false, Ordering::Relaxed);
        self.cancel_all.store(false, Ordering::Relaxed);
        self.cancel_current.store(false, Ordering::Relaxed);
        Ok(self.snapshot())
    }

    fn spawn_worker(&self, app: AppHandle, repo: FaceRepository) {
        let engine = self.clone();
        std::thread::spawn(move || {
            // 批处理使用独立的小缓存：每张开始前清空缓存，
            // 保证同一时刻只有一张全尺寸图（+ 一张渲染结果）驻留内存。
            let cache = ImageCache::with_budget(1, 256 * 1024 * 1024);
            engine.run(Some(&app), &cache, &repo);
        });
    }

    fn claim_next(&self) -> Option<BatchTask> {
        let mut state = self.inner.lock().ok()?;
        let index = state
            .tasks
            .iter()
            .position(|task| task.status == BatchStatus::Pending)?;
        let task = state.tasks[index].clone();
        state.tasks[index].status = BatchStatus::Running;
        state.tasks[index].stage = "准备".into();
        state.tasks[index].progress = 0.01;
        state.tasks[index].started_at_ms = now_ms();
        Some(task)
    }

    fn update<F: FnOnce(&mut BatchTask)>(&self, job_id: &str, apply: F) {
        if let Ok(mut state) = self.inner.lock() {
            if let Some(task) = state.tasks.iter_mut().find(|task| task.job_id == job_id) {
                apply(task);
            }
        }
    }

    fn set_progress(&self, job_id: &str, stage: &str, progress: f32) {
        self.update(job_id, |task| {
            if task.status == BatchStatus::Running {
                task.stage = stage.to_string();
                task.progress = progress.clamp(0.0, 1.0);
            }
        });
    }

    fn run(&self, app: Option<&AppHandle>, cache: &ImageCache, repo: &FaceRepository) {
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        loop {
            if self.cancel_all.load(Ordering::Relaxed) {
                break;
            }
            if self.pause.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            let Some(task) = self.claim_next() else {
                break;
            };
            if self.cancel_all.load(Ordering::Relaxed) {
                self.update(&task.job_id, |item| {
                    item.status = BatchStatus::Cancelled;
                    item.stage = "已取消".into();
                    item.progress = 1.0;
                    item.error = Some("批次已取消，未写入文件".into());
                    item.finished_at_ms = now_ms();
                });
                break;
            }
            self.emit(app);
            self.cancel_current.store(false, Ordering::Relaxed);
            let token = CancelToken::new(self.cancel_current.clone());
            let known_faces: Vec<FaceFrame> = repo
                .get(Path::new(&task.path))
                .map(|entry| entry.faces)
                .unwrap_or_default();

            let result = self.process_task(
                app,
                cache,
                &task,
                &known_faces,
                &token,
                &mut last_emit,
            );

            match result {
                Ok(TaskOutcome::Exported(outcome)) => {
                    self.update(&task.job_id, |item| {
                        item.status = BatchStatus::Completed;
                        item.stage = "完成".into();
                        item.progress = 1.0;
                        item.output_path = Some(outcome.path.to_string_lossy().into_owned());
                        item.bytes = outcome.bytes;
                        item.finished_at_ms = now_ms();
                        item.width = outcome.width;
                        item.height = outcome.height;
                    });
                }
                Ok(TaskOutcome::Skipped(reason)) => {
                    self.update(&task.job_id, |item| {
                        item.status = BatchStatus::Skipped;
                        item.stage = "跳过".into();
                        item.progress = 1.0;
                        item.error = Some(reason);
                        item.finished_at_ms = now_ms();
                    });
                }
                Err(AppError::Cancelled) => {
                    self.update(&task.job_id, |item| {
                        item.status = BatchStatus::Cancelled;
                        item.stage = "已取消".into();
                        item.progress = 1.0;
                        item.error = Some("任务被取消，未写入未完成文件".into());
                        item.finished_at_ms = now_ms();
                    });
                    // 单张取消后继续下一张；整体取消由外层循环终止。
                }
                Err(error) => {
                    let message = error.to_string();
                    self.update(&task.job_id, |item| {
                        item.status = BatchStatus::Failed;
                        item.stage = "失败".into();
                        item.progress = 1.0;
                        item.error = Some(message);
                        item.finished_at_ms = now_ms();
                    });
                }
            }
            self.emit(app);
        }

        // 取消全部：没有开始的任务明确记为 Cancelled，而不是假装完成。
        if self.cancel_all.load(Ordering::Relaxed) {
            if let Ok(mut state) = self.inner.lock() {
                for task in state.tasks.iter_mut() {
                    if matches!(task.status, BatchStatus::Pending | BatchStatus::Running) {
                        task.status = BatchStatus::Cancelled;
                        task.stage = "已取消".into();
                        task.progress = 1.0;
                        task.error = Some("批次已取消，未写入文件".into());
                        task.finished_at_ms = now_ms();
                    }
                }
            }
        }

        if let Ok(mut state) = self.inner.lock() {
            state.running = false;
            state.finished_at_ms = now_ms();
        }
        self.emit(app);
    }

    fn process_task(
        &self,
        app: Option<&AppHandle>,
        cache: &ImageCache,
        task: &BatchTask,
        known_faces: &[FaceFrame],
        token: &CancelToken,
        last_emit: &mut Instant,
    ) -> AppResult<TaskOutcome> {
        // 先释放上一张的全尺寸缓冲，再解码当前照片：两者不并存。
        cache.clear();
        let source = Path::new(&task.path);
        let (working, original, depth) = if crate::image_engine::raw::is_raw_path(source) {
            // RAW 批量：全尺寸显影，与单张导出共用同一函数。
            let params = task
                .raw_params
                .clone()
                .unwrap_or_default()
                .sanitized();
            let developed = crate::image_engine::raw::develop_from_path(source, &params)?;
            (
                std::sync::Arc::new(developed.image),
                source.to_path_buf(),
                crate::image_engine::decoder::BitDepth::Sixteen,
            )
        } else {
            let loaded = cache.load(source)?;
            (loaded.full.clone(), loaded.path.clone(), loaded.depth)
        };
        token.check()?;
        self.set_progress(&task.job_id, "全尺寸渲染", 0.05);
        self.emit(app);

        let job_id = task.job_id.clone();
        let engine = self.clone();
        let app_for_progress = app.cloned();
        let mut progress = |value: f32, stage: &'static str| {
            engine.set_progress(&job_id, stage, 0.05 + value * 0.85);
            if last_emit.elapsed() > Duration::from_millis(120) {
                *last_emit = Instant::now();
                engine.emit(app_for_progress.as_ref());
            }
        };
        let rendered = pipeline::render_with_progress(
            &working,
            &task.params,
            known_faces,
            token,
            &mut progress,
        )?;
        *last_emit = Instant::now();
        token.check()?;

        let (settings, naming, conflict, overwrite_confirmed, output_dir, color) = {
            let state = self.inner.lock().map_err(|_| lock_error())?;
            (
                state.settings,
                state.naming.clone(),
                state.conflict,
                state.overwrite_confirmed,
                state.output_dir.clone(),
                state.color.clone(),
            )
        };

        self.set_progress(&task.job_id, "写出文件", 0.95);
        self.emit(app);
        let destination = match destination_for(
            &output_dir,
            &naming,
            Path::new(&task.path),
            task.number,
            settings.format,
            conflict,
            overwrite_confirmed,
        ) {
            Ok(path) => path,
            Err(reason) => return Ok(TaskOutcome::Skipped(reason)),
        };

        let outcome = export::write_atomic(
            &destination,
            &original,
            &rendered,
            settings,
            depth,
            Some(&color),
        )?;
        Ok(TaskOutcome::Exported(ExportInfo {
            path: outcome.path,
            bytes: outcome.bytes,
            width: rendered.width(),
            height: rendered.height(),
        }))
    }

    fn reset_failed(&self) -> AppResult<()> {
        let mut state = self.inner.lock().map_err(|_| lock_error())?;
        if state.running {
            return Err(AppError::InvalidInput("请先等待当前批处理结束".into()));
        }
        for task in state.tasks.iter_mut() {
            if task.status == BatchStatus::Failed {
                task.reset();
            }
        }
        Ok(())
    }

    fn clear_completed(&self) -> AppResult<()> {
        let mut state = self.inner.lock().map_err(|_| lock_error())?;
        if state.running {
            return Err(AppError::InvalidInput("批处理运行中不能清理任务".into()));
        }
        state
            .tasks
            .retain(|task| !matches!(task.status, BatchStatus::Completed | BatchStatus::Skipped));
        Ok(())
    }
}

enum TaskOutcome {
    Exported(ExportInfo),
    Skipped(String),
}

struct ExportInfo {
    path: PathBuf,
    bytes: u64,
    width: u32,
    height: u32,
}

/// 生成输出路径并处理冲突。默认跳过已有文件；覆盖必须显式确认。
fn destination_for(
    dir: &Path,
    naming: &str,
    source: &Path,
    index: u32,
    format: ExportFormat,
    conflict: ConflictPolicy,
    overwrite_confirmed: bool,
) -> Result<PathBuf, String> {
    let stem = source
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "lumi-export".into());
    let extension = format.extension();
    let rendered = naming
        .replace("{name}", &sanitize(&stem))
        .replace("{index}", &format!("{index:03}"))
        .replace("{format}", extension);
    let mut file_name = sanitize(&rendered);
    if file_name.is_empty() {
        file_name = format!("{}-lumi", sanitize(&stem));
    }
    if Path::new(&file_name).extension().is_none() {
        file_name = format!("{file_name}.{extension}");
    }

    let candidate = dir.join(&file_name);
    if !candidate.exists() {
        return Ok(candidate);
    }
    match conflict {
        ConflictPolicy::Skip => Err("同名文件已存在，按策略跳过".into()),
        ConflictPolicy::Overwrite => {
            if overwrite_confirmed {
                Ok(candidate)
            } else {
                Err("同名文件已存在，需要确认覆盖后才能写入".into())
            }
        }
        ConflictPolicy::Rename => {
            let stem = Path::new(&file_name)
                .file_stem()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| "export".into());
            let extension = format.extension();
            for attempt in 1..=9999 {
                let next = dir.join(format!("{stem}-{attempt}.{extension}"));
                if !next.exists() {
                    return Ok(next);
                }
            }
            Err("无法找到可用的文件名".into())
        }
    }
}

fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| {
            if character.is_control() || "/\\:*?\"<>|".contains(character) {
                '-'
            } else {
                character
            }
        })
        .collect();
    cleaned.trim().trim_matches('-').to_string()
}

#[tauri::command]
pub fn batch_start(
    app: AppHandle,
    repo: State<'_, FaceRepository>,
    engine: State<'_, BatchEngine>,
    request: BatchExportRequest,
) -> AppResult<BatchSnapshot> {
    let snapshot = engine.begin(request)?;
    engine.spawn_worker(app, repo.inner().clone());
    Ok(snapshot)
}

#[tauri::command]
pub fn batch_status(engine: State<'_, BatchEngine>) -> BatchSnapshot {
    engine.snapshot()
}

#[tauri::command]
pub fn batch_pause(engine: State<'_, BatchEngine>) -> BatchSnapshot {
    engine.pause.store(true, Ordering::Relaxed);
    engine.snapshot()
}

#[tauri::command]
pub fn batch_resume(engine: State<'_, BatchEngine>) -> BatchSnapshot {
    engine.pause.store(false, Ordering::Relaxed);
    engine.snapshot()
}

/// 取消当前正在渲染的照片，队列继续处理后面的任务。
#[tauri::command]
pub fn batch_cancel_current(engine: State<'_, BatchEngine>) -> BatchSnapshot {
    engine.cancel_current.store(true, Ordering::Relaxed);
    engine.snapshot()
}

#[tauri::command]
pub fn batch_cancel_all(engine: State<'_, BatchEngine>) -> BatchSnapshot {
    engine.cancel_all.store(true, Ordering::Relaxed);
    // 同时中止当前渲染，避免「取消全部」还要等整张 3000×4000 渲染完。
    engine.cancel_current.store(true, Ordering::Relaxed);
    engine.pause.store(false, Ordering::Relaxed);
    engine.snapshot()
}

#[tauri::command]
pub fn batch_retry_failed(
    app: AppHandle,
    repo: State<'_, FaceRepository>,
    engine: State<'_, BatchEngine>,
) -> AppResult<BatchSnapshot> {
    engine.reset_failed()?;
    let snapshot = engine.snapshot();
    engine.spawn_worker(app, repo.inner().clone());
    Ok(snapshot)
}

#[tauri::command]
pub fn batch_clear_completed(engine: State<'_, BatchEngine>) -> AppResult<BatchSnapshot> {
    engine.clear_completed()?;
    Ok(engine.snapshot())
}

#[tauri::command]
pub fn batch_failures(engine: State<'_, BatchEngine>) -> Vec<BatchTask> {
    engine
        .snapshot()
        .tasks
        .into_iter()
        .filter(|task| task.status == BatchStatus::Failed)
        .collect()
}

#[tauri::command]
pub async fn pick_batch_output_dir() -> AppResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(rfd::FileDialog::new()
            .set_title("选择批量导出目录")
            .pick_folder()
            .map(|path| path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naming_supports_placeholders_and_extension() {
        let dir = std::env::temp_dir().join("lumi-batch-naming-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = destination_for(
            &dir,
            "{name}-{index}",
            Path::new("/photos/人像 A.jpg"),
            7,
            ExportFormat::Webp,
            ConflictPolicy::Skip,
            false,
        )
        .unwrap();
        assert_eq!(path.file_name().unwrap().to_string_lossy(), "人像 A-007.webp");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn existing_file_is_skipped_by_default() {
        let dir = std::env::temp_dir().join("lumi-batch-conflict-test");
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join("photo-lumi.jpg");
        std::fs::write(&existing, b"x").unwrap();
        let result = destination_for(
            &dir,
            "{name}-lumi",
            Path::new("/photos/photo.jpg"),
            1,
            ExportFormat::Jpeg,
            ConflictPolicy::Skip,
            false,
        );
        assert!(result.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_policy_finds_free_suffix() {
        let dir = std::env::temp_dir().join("lumi-batch-rename-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("photo-lumi.jpg"), b"x").unwrap();
        std::fs::write(dir.join("photo-lumi-1.jpg"), b"x").unwrap();
        let path = destination_for(
            &dir,
            "{name}-lumi",
            Path::new("/photos/photo.jpg"),
            1,
            ExportFormat::Jpeg,
            ConflictPolicy::Rename,
            false,
        )
        .unwrap();
        assert_eq!(path.file_name().unwrap().to_string_lossy(), "photo-lumi-2.jpg");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrite_requires_confirmation() {
        let dir = std::env::temp_dir().join("lumi-batch-overwrite-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("photo-lumi.jpg"), b"x").unwrap();
        let denied = destination_for(
            &dir,
            "{name}-lumi",
            Path::new("/photos/photo.jpg"),
            1,
            ExportFormat::Jpeg,
            ConflictPolicy::Overwrite,
            false,
        );
        assert!(denied.is_err());
        let allowed = destination_for(
            &dir,
            "{name}-lumi",
            Path::new("/photos/photo.jpg"),
            1,
            ExportFormat::Jpeg,
            ConflictPolicy::Overwrite,
            true,
        );
        assert!(allowed.is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn formal_pipeline_export_is_atomic_and_never_touches_source() {
        let dir = std::env::temp_dir().join("lumi-batch-atomic-test");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.png");
        let image = image::RgbaImage::from_fn(96, 64, |x, y| {
            image::Rgba([(x * 2) as u8, (y * 3) as u8, 90, 255])
        });
        image::DynamicImage::ImageRgba8(image).save(&source).unwrap();
        let source_bytes = std::fs::read(&source).unwrap();

        let cache = ImageCache::with_budget(1, 64 * 1024 * 1024);
        let loaded = cache.load(&source).unwrap();
        let token = CancelToken::default();
        let rendered = pipeline::render_with_progress(
            &loaded.full,
            &EditParams::default(),
            &[],
            &token,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!((rendered.width(), rendered.height()), (96, 64));

        let destination = dir.join("out.jpg");
        let first = export::write_atomic(
            &destination,
            &loaded.path,
            &rendered,
            ExportSettings {
                format: ExportFormat::Jpeg,
                quality: 90,
            },
            loaded.depth,
            None,
        )
        .unwrap();
        assert!(first.bytes > 0);
        assert!(destination.exists());
        assert_eq!(std::fs::read(&source).unwrap(), source_bytes, "源图必须保持不变");

        // 覆盖写同样走临时文件 + 重命名，不留下半成品。
        export::write_atomic(
            &destination,
            &loaded.path,
            &rendered,
            ExportSettings {
                format: ExportFormat::Jpeg,
                quality: 88,
            },
            loaded.depth,
            None,
        )
        .unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".lumi-tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "不应残留临时文件");
        std::fs::remove_dir_all(&dir).ok();
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumi-batch-{tag}-{}", now_ms()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        let image = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 251) as u8, (y % 241) as u8, 120, 255])
        });
        image::DynamicImage::ImageRgba8(image).save(path).unwrap();
    }

    fn make_request(photos: Vec<PathBuf>, output: &Path) -> BatchExportRequest {
        BatchExportRequest {
            photos: photos
                .into_iter()
                .map(|path| BatchPhoto {
                    path: path.to_string_lossy().into_owned(),
                    params: EditParams::default(),
                    raw_params: None,
                })
                .collect(),
            settings: ExportSettings {
                format: ExportFormat::Jpeg,
                quality: 90,
            },
            output_dir: output.to_string_lossy().into_owned(),
            naming: "{name}-out".into(),
            start_index: 1,
            conflict: ConflictPolicy::Skip,
            overwrite_confirmed: false,
        }
    }

    fn output_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|kind| kind.is_file()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn batch_exports_every_photo_and_isolates_broken_file() {
        let dir = temp_dir("queue-mixed");
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let first = dir.join("first.png");
        let second = dir.join("second.png");
        let broken = dir.join("broken.png");
        write_png(&first, 120, 80);
        write_png(&second, 64, 48);
        std::fs::write(&broken, b"this is not an image").unwrap();
        let source_bytes = std::fs::read(&first).unwrap();

        let engine = BatchEngine::default();
        engine
            .begin(make_request(
                vec![first.clone(), broken.clone(), second.clone()],
                &out,
            ))
            .unwrap();
        let cache = ImageCache::with_budget(1, 64 * 1024 * 1024);
        engine.run(None, &cache, &FaceRepository::default());

        let snapshot = engine.snapshot();
        assert!(!snapshot.running);
        assert_eq!(snapshot.total, 3);
        assert_eq!(snapshot.completed, 2, "两张正常照片必须导出成功");
        assert_eq!(snapshot.failed, 1, "坏图单独失败，不拖垮整批");
        assert_eq!(snapshot.exported, 2);
        assert!(snapshot.tasks.iter().any(|task| {
            task.name == "broken.png" && task.status == BatchStatus::Failed && task.error.is_some()
        }));
        assert_eq!(output_files(&out).len(), 2);
        assert_eq!(std::fs::read(&first).unwrap(), source_bytes, "源图必须保持不变");
        let leftovers = std::fs::read_dir(&out)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".lumi-tmp-"))
            .count();
        assert_eq!(leftovers, 0, "失败任务不得留下临时文件");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mixed_sizes_and_formats_export_into_new_directory() {
        let dir = temp_dir("queue-mixed-formats");
        let out = dir.join("not-created-yet");
        let mut photos = Vec::new();
        for index in 0..10u32 {
            let (width, height) = match index % 3 {
                0 => (96, 64),
                1 => (72, 90),
                _ => (120, 80),
            };
            let image = image::RgbaImage::from_fn(width, height, |x, y| {
                image::Rgba([(x % 251) as u8, (y % 241) as u8, 90, 255])
            });
            let path = if index % 2 == 0 {
                let path = dir.join(format!("mixed-{index:02}.png"));
                image::DynamicImage::ImageRgba8(image).save(&path).unwrap();
                path
            } else {
                let path = dir.join(format!("mixed-{index:02}.jpg"));
                image::DynamicImage::ImageRgba8(image)
                    .to_rgb8()
                    .save(&path)
                    .unwrap();
                path
            };
            photos.push(path);
        }

        let engine = BatchEngine::default();
        engine.begin(make_request(photos, &out)).unwrap();
        let cache = ImageCache::with_budget(1, 64 * 1024 * 1024);
        engine.run(None, &cache, &FaceRepository::default());

        let snapshot = engine.snapshot();
        assert_eq!(snapshot.total, 10);
        assert_eq!(snapshot.completed, 10);
        assert_eq!(snapshot.failed, 0);
        assert!(out.is_dir(), "输出目录不存在时应自动创建");
        assert_eq!(output_files(&out).len(), 10);
        for entry in output_files(&out) {
            assert!(entry.ends_with(".jpg"), "输出必须使用请求的格式：{entry}");
            let image = image::open(out.join(&entry)).unwrap();
            assert!(image.width() > 0 && image.height() > 0);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancel_all_marks_tasks_cancelled_without_partial_outputs() {
        let dir = temp_dir("queue-cancel-all");
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let first = dir.join("a.png");
        let second = dir.join("b.png");
        write_png(&first, 200, 140);
        write_png(&second, 200, 140);

        let engine = BatchEngine::default();
        engine
            .begin(make_request(vec![first, second], &out))
            .unwrap();
        engine.cancel_all.store(true, Ordering::Relaxed);
        let cache = ImageCache::with_budget(1, 64 * 1024 * 1024);
        engine.run(None, &cache, &FaceRepository::default());

        let snapshot = engine.snapshot();
        assert!(!snapshot.running);
        assert_eq!(snapshot.cancelled, snapshot.total);
        assert_eq!(snapshot.completed, 0);
        assert!(output_files(&out).is_empty(), "取消后不得写出任何文件");
        assert!(snapshot
            .tasks
            .iter()
            .all(|task| task.error.as_deref() == Some("批次已取消，未写入文件")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn retry_failed_only_resets_failed_tasks_and_reports_partial_success() {
        let dir = temp_dir("queue-retry");
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let good = dir.join("good.png");
        let broken = dir.join("later.png");
        write_png(&good, 96, 72);
        std::fs::write(&broken, b"broken").unwrap();

        let engine = BatchEngine::default();
        engine
            .begin(make_request(vec![good, broken.clone()], &out))
            .unwrap();
        let cache = ImageCache::with_budget(1, 64 * 1024 * 1024);
        engine.run(None, &cache, &FaceRepository::default());
        let first = engine.snapshot();
        assert_eq!((first.completed, first.failed), (1, 1), "部分成功必须可识别");
        assert_eq!(output_files(&out).len(), 1);

        // 用户修复坏图后单独重试失败项，已完成文件不重复写。
        write_png(&broken, 80, 60);
        engine.reset_failed().unwrap();
        let pending = engine.snapshot();
        assert_eq!(pending.failed, 0);
        assert_eq!(pending.pending, 1);
        assert_eq!(pending.completed, 1, "已完成任务不得被重置");

        let retry_cache = ImageCache::with_budget(1, 64 * 1024 * 1024);
        engine.run(None, &retry_cache, &FaceRepository::default());
        let second = engine.snapshot();
        assert_eq!(second.completed, 2);
        assert_eq!(second.failed, 0);
        assert_eq!(output_files(&out).len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn existing_output_is_skipped_and_never_overwritten() {
        let dir = temp_dir("queue-skip");
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let source = dir.join("photo.png");
        write_png(&source, 64, 48);
        let existing = out.join("photo-out.jpg");
        std::fs::write(&existing, b"keep-me").unwrap();

        let engine = BatchEngine::default();
        engine.begin(make_request(vec![source], &out)).unwrap();
        let cache = ImageCache::with_budget(1, 32 * 1024 * 1024);
        engine.run(None, &cache, &FaceRepository::default());

        let snapshot = engine.snapshot();
        assert_eq!(snapshot.skipped, 1);
        assert_eq!(snapshot.completed, 0);
        assert_eq!(std::fs::read(&existing).unwrap(), b"keep-me", "默认策略不得覆盖已有文件");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn batch_export_matches_single_export_byte_for_byte() {
        let dir = temp_dir("queue-consistency");
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let source = dir.join("consistency.png");
        write_png(&source, 200, 150);

        let preset = crate::image_engine::preset::builtin_presets()
            .into_iter()
            .find(|item| item.id == "builtin-warm-skin")
            .unwrap();
        let (params, _) = crate::image_engine::preset::apply_preset(
            &EditParams::default(),
            &preset,
            65.0,
            &[],
        );

        let engine = BatchEngine::default();
        let mut request = make_request(vec![source.clone()], &out);
        request.photos[0].params = params.clone();
        engine.begin(request).unwrap();
        let cache = ImageCache::with_budget(1, 64 * 1024 * 1024);
        engine.run(None, &cache, &FaceRepository::default());
        assert_eq!(engine.snapshot().completed, 1);
        let batch_file = out.join("consistency-out.jpg");
        let batch_bytes = std::fs::read(&batch_file).unwrap();

        // 单张导出：同一份参数、同一渲染管线、同一编码设置。
        let loaded = cache.load(&source).unwrap();
        let rendered = pipeline::render(
            &loaded.full,
            &params,
            &[],
            &CancelToken::default(),
        )
        .unwrap();
        let single = dir.join("single.jpg");
        let color = crate::color_management::ColorOutputSettings::default();
        export::write_atomic(
            &single,
            &loaded.path,
            &rendered,
            ExportSettings {
                format: ExportFormat::Jpeg,
                quality: 90,
            },
            loaded.depth,
            Some(&color),
        )
        .unwrap();
        assert_eq!(
            batch_bytes,
            std::fs::read(&single).unwrap(),
            "批量与单张导出必须逐字节一致"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 性能基线：默认忽略，需手动执行
    /// `cargo test --release --lib round6_performance -- --ignored --nocapture`。
    /// 输出的是本机真实测量值，不参与常规测试，避免拖慢 CI。
    #[test]
    #[ignore = "性能基线，需要 release 构建手动运行"]
    fn round6_performance_baseline() {
        let dir = temp_dir("perf");

        let params = {
            let (mut base, _) = crate::image_engine::preset::apply_preset(
                &EditParams::default(),
                &crate::image_engine::preset::builtin_presets()
                    .into_iter()
                    .find(|item| item.id == "builtin-natural-portrait")
                    .unwrap(),
                100.0,
                &[],
            );
            base.sharpness = 18.0;
            base.clarity = 12.0;
            base
        };

        for (width, height) in [(1600u32, 1200u32), (3000, 4000)] {
            let source = dir.join(format!("single-{width}x{height}.png"));
            write_png(&source, width, height);
            let cache = ImageCache::with_budget(1, 640 * 1024 * 1024);
            let loaded = cache.load(&source).unwrap();
            let token = CancelToken::default();
            let started = Instant::now();
            let rendered = pipeline::render(&loaded.full, &params, &[], &token).unwrap();
            let elapsed = started.elapsed().as_secs_f64();
            eprintln!(
                "PERF single {width}x{height}: {:.3}s ({}x{})",
                elapsed,
                rendered.width(),
                rendered.height()
            );
        }

        for count in [10usize, 50] {
            let batch_dir = dir.join(format!("batch-{count}"));
            std::fs::create_dir_all(&batch_dir).unwrap();
            let mut photos = Vec::new();
            for index in 0..count {
                let path = batch_dir.join(format!("photo-{index:02}.png"));
                if index == 0 {
                    write_png(&path, 1600, 1200);
                } else {
                    // 其余照片复用同一份字节，避免测试准备时间掩盖队列性能。
                    std::fs::copy(batch_dir.join("photo-00.png"), &path).unwrap();
                }
                photos.push(path);
            }
            let output = dir.join(format!("out-{count}"));
            std::fs::create_dir_all(&output).unwrap();
            let engine = BatchEngine::default();
            let mut request = make_request(photos, &output);
            for photo in request.photos.iter_mut() {
                photo.params = params.clone();
            }
            engine.begin(request).unwrap();
            let cache = ImageCache::with_budget(1, 640 * 1024 * 1024);
            let started = Instant::now();
            engine.run(None, &cache, &FaceRepository::default());
            let elapsed = started.elapsed().as_secs_f64();
            let snapshot = engine.snapshot();
            eprintln!(
                "PERF batch {count}x1600x1200: {:.2}s total, {:.3}s/photo, completed={}, failed={}",
                elapsed,
                elapsed / count as f64,
                snapshot.completed,
                snapshot.failed
            );
            assert_eq!(snapshot.failed, 0);
        }

        // 取消响应：大批次运行中触发取消，记录从发出取消到队列停止的时间。
        let cancel_dir = dir.join("cancel");
        std::fs::create_dir_all(&cancel_dir).unwrap();
        let mut photos = Vec::new();
        for index in 0..10 {
            let path = cancel_dir.join(format!("cancel-{index:02}.png"));
            if index == 0 {
                write_png(&path, 3000, 4000);
            } else {
                std::fs::copy(cancel_dir.join("cancel-00.png"), &path).unwrap();
            }
            photos.push(path);
        }
        let cancel_out = dir.join("cancel-out");
        std::fs::create_dir_all(&cancel_out).unwrap();
        let engine = BatchEngine::default();
        let mut request = make_request(photos, &cancel_out);
        for photo in request.photos.iter_mut() {
            photo.params = params.clone();
        }
        engine.begin(request).unwrap();
        let worker_engine = engine.clone();
        let worker = std::thread::spawn(move || {
            let cache = ImageCache::with_budget(1, 640 * 1024 * 1024);
            worker_engine.run(None, &cache, &FaceRepository::default());
        });
        std::thread::sleep(Duration::from_millis(400));
        let cancel_started = Instant::now();
        engine.cancel_all.store(true, Ordering::Relaxed);
        engine.cancel_current.store(true, Ordering::Relaxed);
        worker.join().unwrap();
        let cancel_elapsed = cancel_started.elapsed().as_secs_f64();
        let snapshot = engine.snapshot();
        eprintln!(
            "PERF cancel 10x3000x4000: {:.3}s, completed={}, cancelled={}",
            cancel_elapsed, snapshot.completed, snapshot.cancelled
        );
        assert!(!snapshot.running);
        assert_eq!(
            snapshot.cancelled + snapshot.completed,
            snapshot.total,
            "取消后每个任务都必须处于终态"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn preset_zero_and_full_strength_produce_expected_render_equivalence() {
        let dir = std::env::temp_dir().join("lumi-batch-preset-test");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.png");
        let image = image::RgbaImage::from_fn(64, 48, |x, y| {
            image::Rgba([(x * 3) as u8, 140, (y * 4) as u8, 255])
        });
        image::DynamicImage::ImageRgba8(image).save(&source).unwrap();
        let cache = ImageCache::with_budget(1, 32 * 1024 * 1024);
        let loaded = cache.load(&source).unwrap();
        let token = CancelToken::default();
        let base = EditParams::default();
        let preset = crate::image_engine::preset::builtin_presets()
            .into_iter()
            .find(|item| item.id == "builtin-cinematic-teal")
            .unwrap();

        let (zero, _) =
            crate::image_engine::preset::apply_preset(&base, &preset, 0.0, &[]);
        let plain = pipeline::render(&loaded.full, &base, &[], &token).unwrap();
        let zero_rendered = pipeline::render(&loaded.full, &zero, &[], &token).unwrap();
        assert_eq!(plain, zero_rendered, "0% 强度必须等同于不应用预设");

        let (full, _) =
            crate::image_engine::preset::apply_preset(&base, &preset, 100.0, &[]);
        let full_rendered = pipeline::render(&loaded.full, &full, &[], &token).unwrap();
        assert_ne!(plain, full_rendered, "100% 强度必须真实改变像素");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 手工基准：50 张 1600×1200 的批量导出，用于复核内存峰值。
    /// `cargo test --release batch_memory_bench -- --ignored --nocapture`
    #[test]
    #[ignore = "manual memory benchmark"]
    fn batch_memory_bench() {
        let count: usize = std::env::var("LUMI_BENCH_COUNT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(50);
        let dir = temp_dir("mem-bench");
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let mut photos = Vec::with_capacity(count);
        for index in 0..count {
            let source = dir.join(format!("bench-{index:03}.jpg"));
            let image = image::RgbaImage::from_fn(1600, 1200, |x, y| {
                image::Rgba([
                    (x.wrapping_add(index as u32 * 7) % 256) as u8,
                    (y.wrapping_add(index as u32 * 3) % 256) as u8,
                    ((x + y) % 256) as u8,
                    255,
                ])
            });
            image::DynamicImage::ImageRgba8(image)
                .to_rgb8()
                .save(&source)
                .unwrap();
            photos.push(source);
        }

        let engine = BatchEngine::default();
        engine.begin(make_request(photos, &out)).unwrap();
        let cache = ImageCache::with_budget(1, 256 * 1024 * 1024);
        let started = Instant::now();
        engine.run(None, &cache, &FaceRepository::default());
        let snapshot = engine.snapshot();
        println!(
            "BENCH count={} completed={} failed={} elapsed_ms={} exported_bytes={}",
            count,
            snapshot.completed,
            snapshot.failed,
            started.elapsed().as_millis(),
            snapshot
                .tasks
                .iter()
                .map(|task| task.bytes)
                .sum::<u64>()
        );
        assert_eq!(snapshot.completed, count);
        std::fs::remove_dir_all(&dir).ok();
    }
}
