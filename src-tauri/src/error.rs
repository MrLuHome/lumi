use serde::ser::{Serialize, Serializer};

/// 命令层统一错误类型。序列化为纯字符串，前端可直接展示。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("不支持的图片格式：{0}")]
    Unsupported(String),

    #[error("图片解码失败：{0}")]
    Decode(String),

    #[error("图片编码失败：{0}")]
    Encode(String),

    #[error("没有找到可导入的图片（支持 JPG / PNG / WebP / TIFF）")]
    EmptySelection,

    #[error("后台任务执行失败：{0}")]
    Task(String),

    #[error("参数无效：{0}")]
    InvalidInput(String),

    #[error("导出已取消")]
    Cancelled,
}

pub type AppResult<T> = Result<T, AppError>;

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        match err {
            image::ImageError::Unsupported(inner) => AppError::Unsupported(inner.to_string()),
            image::ImageError::IoError(inner) => AppError::Io(inner),
            other => AppError::Decode(other.to_string()),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
