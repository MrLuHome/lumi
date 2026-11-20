use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{AppError, AppResult};

/// 渲染任务取消标志。前端只保留最新请求，旧任务在阶段边界主动退出，
/// 避免拖动滑杆时越积越多的后台计算。
#[derive(Clone, Default)]
pub struct CancelToken {
    flag: Option<Arc<AtomicBool>>,
}

impl CancelToken {
    pub fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag: Some(flag) }
    }

    pub fn check(&self) -> AppResult<()> {
        if let Some(flag) = &self.flag {
            if flag.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
        }
        Ok(())
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag
            .as_ref()
            .map(|flag| flag.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
}
