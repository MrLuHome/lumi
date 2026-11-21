#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 模型隔离探针：应用自身以子进程方式运行，只做 ONNX 结构与冒烟验证，
    // 不初始化任何窗口/图形上下文。必须在 Tauri 启动之前分流。
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(String::as_str)
        == Some(lumi_retouch_lib::face_intelligence::model_hub::probe::PROBE_FLAG)
    {
        std::process::exit(lumi_retouch_lib::face_intelligence::model_hub::probe::run_cli(&argv));
    }
    lumi_retouch_lib::run()
}
