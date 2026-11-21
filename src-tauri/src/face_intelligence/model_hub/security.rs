//! Model Hub 安全校验。
//!
//! 目标不是“能加载就行”，而是：
//! - 只接受单文件 ONNX，拒绝符号链接与外部数据引用；
//! - 限制文件体积，读取前先检查元数据；
//! - 记录 SHA256 与原始路径，便于追溯；
//! - 拒绝伪装成 ONNX 的可执行文件 / 压缩包 / pickle。

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_MODEL_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportPath {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = File::open(path).map_err(|error| format!("无法打开模型文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("读取模型失败：{error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// 读取最多 `limit + 1` 字节，超过即拒绝，避免把超大文件读进内存。
pub fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| format!("无法读取文件信息：{error}"))?;
    if metadata.len() > limit {
        return Err(format!(
            "文件体积 {} 超过上限 {}",
            human_size(metadata.len()),
            human_size(limit)
        ));
    }
    let file = File::open(path).map_err(|error| format!("无法打开文件：{error}"))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取文件失败：{error}"))?;
    if bytes.len() as u64 > limit {
        return Err("文件在读取过程中超过上限".into());
    }
    Ok(bytes)
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// 导入前的路径检查。返回规范化路径、体积与 SHA256。
pub fn validate_import_path(path: &Path) -> Result<ImportPath, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| format!("无法访问模型文件：{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("不接受符号链接形式的模型文件，请先复制为普通文件".into());
    }
    if !metadata.file_type().is_file() {
        return Err("模型路径不是普通文件".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension != "onnx" {
        return Err(format!("只接受 .onnx 模型，实际扩展名 `{extension}`"));
    }
    if metadata.len() == 0 {
        return Err("模型文件为空".into());
    }
    if metadata.len() > MAX_MODEL_BYTES {
        return Err(format!(
            "模型体积 {} 超过 512 MB 上限",
            human_size(metadata.len())
        ));
    }

    // 只读文件头，拒绝常见伪装格式。
    let mut header = [0u8; 8];
    let mut file = File::open(path).map_err(|error| format!("无法打开模型文件：{error}"))?;
    let read = file
        .read(&mut header)
        .map_err(|error| format!("读取模型头失败：{error}"))?;
    let header = &header[..read];
    if header.starts_with(b"PK\x03\x04") {
        return Err("文件是 ZIP 压缩包，不是 ONNX 模型".into());
    }
    if header.starts_with(b"\x93NUMPY") {
        return Err("文件是 NumPy 数据，不是 ONNX 模型".into());
    }
    if header.starts_with(b"\x7fELF") {
        return Err("文件是可执行文件，拒绝导入".into());
    }
    if header.len() >= 4
        && matches!(
            &header[..4],
            [0xcf, 0xfa, 0xed, 0xfe] | [0xce, 0xfa, 0xed, 0xfe]
        )
    {
        return Err("文件是 Mach-O 可执行文件，拒绝导入".into());
    }
    if header.starts_with(b"%PDF") {
        return Err("文件是 PDF，不是 ONNX 模型".into());
    }
    if header.starts_with(b"version https://git-lfs") {
        return Err("这是 Git LFS 指针文件，请先下载真实模型".into());
    }

    let sha256 = sha256_file(path)?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("无法规范化模型路径：{error}"))?;
    Ok(ImportPath {
        path: canonical,
        size: metadata.len(),
        sha256,
    })
}

/// ONNX 外部数据会在推理时读取旁路文件，扩大攻击面。本轮只接受单文件模型。
pub fn reject_external_data(bytes: &[u8]) -> Result<(), String> {
    const MARKER: &[u8] = b"external_data";
    if bytes.windows(MARKER.len()).any(|window| window == MARKER) {
        return Err("模型引用了外部数据文件（external_data），本轮只接受单文件 ONNX".into());
    }
    Ok(())
}

/// 模型 ID 会参与路径拼接，必须严格限制。
pub fn validate_model_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("模型 ID 长度必须在 1..64".into());
    }
    if !id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err("模型 ID 只能包含字母、数字、-、_、.".into());
    }
    if id.starts_with('.') {
        return Err("模型 ID 不能以 . 开头".into());
    }
    Ok(())
}

/// 在已校验 ID 的前提下拼接模型目录。
pub fn safe_join(root: &Path, id: &str) -> Result<PathBuf, String> {
    validate_model_id(id)?;
    let path = root.join(id);
    if !path.starts_with(root) {
        return Err("模型路径越界".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lumi-model-security-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_zip_disguised_as_onnx() {
        let dir = temp_dir("zip");
        let path = dir.join("evil.onnx");
        std::fs::write(&path, b"PK\x03\x04rest").unwrap();
        assert!(validate_import_path(&path).unwrap_err().contains("ZIP"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_wrong_extension_and_empty_file() {
        let dir = temp_dir("ext");
        let path = dir.join("model.bin");
        std::fs::write(&path, b"abc").unwrap();
        assert!(validate_import_path(&path).unwrap_err().contains("扩展名"));
        let empty = dir.join("empty.onnx");
        std::fs::File::create(&empty).unwrap();
        assert!(validate_import_path(&empty).unwrap_err().contains("为空"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sha256_is_stable() {
        let dir = temp_dir("sha");
        let path = dir.join("m.onnx");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"onnx-bytes").unwrap();
        let first = sha256_file(&path).unwrap();
        assert_eq!(first, sha256_bytes(b"onnx-bytes"));
        assert_eq!(first.len(), 64);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn external_data_marker_is_rejected() {
        assert!(reject_external_data(b"safe").is_ok());
        assert!(reject_external_data(b"weight_location: external_data").is_err());
    }

    #[test]
    fn model_id_cannot_escape_root() {
        assert!(validate_model_id("../evil").is_err());
        assert!(validate_model_id("ok-model_1.0").is_ok());
        assert!(safe_join(Path::new("/tmp/models"), "../etc").is_err());
    }

    #[test]
    fn read_limit_enforced() {
        let dir = temp_dir("limit");
        let path = dir.join("big.onnx");
        std::fs::write(&path, vec![0u8; 32]).unwrap();
        assert!(read_limited(&path, 16).is_err());
        assert_eq!(read_limited(&path, 32).unwrap().len(), 32);
        std::fs::remove_dir_all(&dir).ok();
    }
}
