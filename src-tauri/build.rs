use std::path::PathBuf;
use std::process::Command;

fn main() {
    tauri_build::build();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_vision_bridge();
    }
}

/// Apple Vision 桥接用 Objective-C 编写，系统框架不提供 Rust 绑定。
/// 这里要求构建机具备 Xcode Command Line Tools（clang / ar），
/// 缺失时直接失败并给出明确原因，不静默降级成“Provider 不可用”。
fn build_vision_bridge() {
    let source = PathBuf::from("src/face_intelligence/providers/vision_bridge.m");
    if !source.is_file() {
        panic!("缺少 Apple Vision 桥接源文件：{}", source.display());
    }
    println!("cargo:rerun-if-changed={}", source.display());

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR 未设置"));
    let object = out_dir.join("vision_bridge.o");
    let archive = out_dir.join("liblumi_vision_bridge.a");

    let status = Command::new("clang")
        .args(["-fobjc-arc", "-fmodules", "-O2", "-c"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .status()
        .expect("需要 clang 编译 Apple Vision 桥接，请安装 Xcode Command Line Tools");
    assert!(status.success(), "vision_bridge.m 编译失败");

    let status = Command::new("ar")
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("需要 ar 打包 Apple Vision 静态库");
    assert!(status.success(), "liblumi_vision_bridge.a 打包失败");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=lumi_vision_bridge");
    println!("cargo:rustc-link-lib=framework=Vision");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=objc");
}
