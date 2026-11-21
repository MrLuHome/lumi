#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 模型隔离子进程：应用自身以 Worker 方式运行，只做 ONNX 结构/冒烟验证
    // 或真实关键点推理，不初始化任何窗口/图形上下文。必须在 Tauri 启动之前分流。
    let argv: Vec<String> = std::env::args().collect();
    let probe = lumi_retouch_lib::face_intelligence::model_hub::probe::PROBE_FLAG;
    let infer = lumi_retouch_lib::face_intelligence::model_hub::probe::INFER_FLAG;
    match argv.get(1).map(String::as_str) {
        Some(flag) if flag == probe || flag == infer => {
            std::process::exit(
                lumi_retouch_lib::face_intelligence::model_hub::probe::run_cli(&argv),
            );
        }
        _ => {}
    }
    lumi_retouch_lib::run()
}
