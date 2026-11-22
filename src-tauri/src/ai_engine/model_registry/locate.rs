//! 权重定位（纯函数，不碰 `AppHandle`）。
//!
//! 边界铁律：`resource_dir()` 只能在 `commands/` 层调用。命令层把
//! `AppHandle` 解析成一串基础目录后传进来，`ai_engine` 因此可以脱离 Tauri 单测。
//!
//! 候选顺序与既有实现完全一致（`commands/faces.rs::model_path` 的双候选），
//! 再追加仓库内 `resources/` 作为开发期回退。

use std::path::{Path, PathBuf};

/// 生成候选路径列表：`<base>/<relative_dir>/<file>`。
pub fn candidates(base_dirs: &[PathBuf], relative_dir: &str, file: &str) -> Vec<PathBuf> {
    let relative = Path::new(relative_dir);
    base_dirs
        .iter()
        .map(|base| base.join(relative).join(file))
        .collect()
}

/// 返回第一个真实存在的候选。存在性判断只做一次 `metadata`，不读内容。
pub fn locate(base_dirs: &[PathBuf], relative_dir: &str, file: &str) -> Option<PathBuf> {
    candidates(base_dirs, relative_dir, file)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// 开发期回退目录（仓库内的 `resources/`）。
///
/// 打包后 `resource_dir()` 会先行命中，这里的候选只是让 `cargo test` 与
/// `pnpm tauri dev` 在未打包时也能找到权重。
pub fn dev_base_dirs() -> Vec<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    vec![manifest.join("resources"), manifest]
}

/// 把候选路径转成字符串列表，用于失败态的 `searched` 字段。
pub fn describe(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lumi-locate-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn locate_returns_the_first_existing_candidate() {
        let root = temp_dir("first");
        let nested = root.join("models/yunet");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("m.onnx");
        std::fs::write(&file, b"x").unwrap();

        let found = locate(std::slice::from_ref(&root), "models/yunet", "m.onnx");
        assert_eq!(found.as_deref(), Some(file.as_path()));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn locate_returns_none_when_no_candidate_exists() {
        let root = temp_dir("none");
        assert!(locate(std::slice::from_ref(&root), "models/yunet", "missing.onnx").is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn candidates_preserve_order_and_cover_both_resource_layouts() {
        let base = PathBuf::from("/app/Contents/Resources");
        let list = candidates(
            &[base.join("resources"), base.clone()],
            "models/yunet",
            "m.onnx",
        );
        assert_eq!(list.len(), 2);
        assert!(list[0].to_string_lossy().contains("Resources/resources/models/yunet"));
        assert!(list[1].to_string_lossy().contains("Resources/models/yunet"));
    }

    #[test]
    fn locate_is_a_pure_function_without_tauri() {
        // 只依赖 `&[PathBuf]`：没有任何 Tauri 上下文也能断言行为。
        assert!(locate(&[], "models/yunet", "m.onnx").is_none());
        assert!(candidates(&[], "models/yunet", "m.onnx").is_empty());
    }

    #[test]
    fn describe_returns_all_searched_paths() {
        let described = describe(&[PathBuf::from("/a/b"), PathBuf::from("/c/d")]);
        assert_eq!(described, vec!["/a/b".to_string(), "/c/d".to_string()]);
    }
}
