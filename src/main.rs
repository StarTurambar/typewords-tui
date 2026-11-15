use clap::Parser;

fn main() -> std::process::ExitCode {
    // 第一件事装崩溃报告钩子：参数解析、路径解析、TUI 里任何一处 panic
    // 都能留下 crash-<时间戳>.txt，而不是只在 stderr 闪一下。
    typewords_tui::crash::install_hook();
    let cli = typewords_tui::cli::Cli::parse();
    match typewords_tui::run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            typewords_tui::crash::report_fatal_error(&err);
            // Windows 双击启动时窗口会随进程退出而关闭，等用户看清错误再走。
            typewords_tui::crash::wait_for_enter_on_windows();
            std::process::ExitCode::FAILURE
        }
    }
}
