//! 可视化调试工具：把练习页渲染成文本打印到 stdout（开发/截图用）。
//!
//! 用法：cargo run --example dump -- [mode] [typed] [width] [height] [big|plain]
//!   mode  : system/free/review/shuffle/identify/dictation/listen
//!   typed : 预先键入的字符（用于检查已输入/错误状态）
//!   big   : big=2 倍大字（默认），plain=强制 1 倍

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use typewords_tui::app;
use typewords_tui::cli::Cli;
use typewords_tui::paths::Paths;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).cloned().unwrap_or_else(|| "free".to_string());
    let typed = args.get(2).cloned().unwrap_or_default();
    let width: u16 = args
        .get(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(120);
    let height: u16 = args
        .get(4)
        .and_then(|value| value.parse().ok())
        .unwrap_or(40);
    let big = args.get(5).map(|value| value != "plain").unwrap_or(true);

    let Some(assets) = typewords_tui::paths::locate_assets(None) else {
        eprintln!(
            "找不到词库资源目录：请在仓库根运行（需要 assets/list/word.json），或设置 TYPEWORDS_ASSETS 指向资源目录"
        );
        std::process::exit(1);
    };
    let home = std::env::temp_dir().join(format!("tw-dump-home-{mode}"));
    let _ = std::fs::remove_dir_all(&home);
    let paths = Paths {
        home,
        assets: assets.clone(),
    };
    let cli = Cli {
        assets: Some(assets),
        home: Some(paths.home.clone()),
        dict: None,
        mode: Some(mode.clone()),
        theme: None,
        seed: Some(7),
        reset_practice: false,
        command: None,
    };
    let mut app = app::build_for_test(paths, cli).expect("app");
    app.boot.settings.tui_big_word = big;
    app.start_practice();
    for character in typed.chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    println!("===== mode={mode} typed={typed:?} {width}x{height} big={big} =====");
    for y in 0..height {
        let mut line = String::new();
        let mut skip = 0usize;
        for x in 0..width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                let symbol = cell.symbol();
                line.push_str(symbol);
                let cell_width = unicode_width::UnicodeWidthStr::width(symbol);
                if cell_width > 1 {
                    skip = cell_width - 1;
                }
            }
        }
        println!("{line}");
    }
}
