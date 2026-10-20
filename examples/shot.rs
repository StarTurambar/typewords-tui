//! 通用 TUI 截图工具：把任意页面用 ratatui TestBackend 渲染一遍，输出带样式的
//! JSON 单元格网格；配合 `tui/tools/render-snapshot.py` 生成 PNG。--plain 则输出
//! 等宽纯文本，便于终端里快速对比。
//!
//! 用法：
//!   cargo run --quiet --example shot -- <screen> [width] [height] [theme] [--plain]
//!
//!   screen: home | practice | dict | stats | settings | help | summary | edit | confirm
//!   width/height: 终端列/行，默认 120 x 40
//!   theme: 写入 app.boot.settings.theme 的字符串（dark/light/high-contrast，
//!          重设计后的自定义主题名也可传；未知名字由 app 自行回退）
//!
//! 例：
//!   cargo run --quiet --example shot -- home 120 40 dark > %TEMP%\\home.json
//!   python tui/tools/render-snapshot.py %TEMP%\\home.json
//!
//! JSON 结构：
//!   {"width":W,"height":H,"screen":"home","theme":"dark","rows":[
//!     [{"sym":" ", "fg":"reset","bg":"reset","bold":false,
//!       "reversed":false,"underline":false,"width":1}, ...], ...]}
//!
//! 颜色序列化规则：`"reset"` | 命名色（black/red/.../darkgray/lightcyan/...）|
//! `"#rrggbb"` | `"indexed:N"`。宽字符（CJK）占两列，后随的续列为空格占位，
//! 渲染器按 width 跳过续列。
//!
//! 页面/弹层都通过公开 API（build_for_test + on_key + render + pub 字段 screen）
//! 进入，不构造内部弹层结构体，尽量不受 UI 重设计影响。

use anyhow::{Context, Result, bail};
use clap::Parser;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier};
use serde_json::{Value, json};
use unicode_width::UnicodeWidthStr;

use typewords_tui::app::{self, App, SETTING_ROWS, Screen};
use typewords_tui::cli::Cli;
use typewords_tui::paths::{Paths, locate_assets};

const SUPPORTED_SCREENS: &[&str] = &[
    "home",
    "practice",
    "dict",
    "stats",
    "settings",
    "shortcuts",
    "help",
    "summary",
    "edit",
    "confirm",
];

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut plain = false;
    let mut positional: Vec<String> = Vec::new();
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--plain" => plain = true,
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            other if other.starts_with("--") => {
                bail!("unknown flag {other:?}（--plain is the only flag）");
            }
            other => positional.push(other.to_string()),
        }
    }

    let screen = positional
        .first()
        .cloned()
        .unwrap_or_else(|| "home".to_string());
    if !SUPPORTED_SCREENS.contains(&screen.as_str()) {
        bail!(
            "unknown screen {screen:?}; supported: {}",
            SUPPORTED_SCREENS.join(", ")
        );
    }
    let width = parse_dimension(positional.get(1), 120, "width")?;
    let height = parse_dimension(positional.get(2), 40, "height")?;
    let theme = positional
        .get(3)
        .cloned()
        .unwrap_or_else(|| "dark".to_string());

    let assets = locate_assets(None)
        .context("assets dir not found：请在仓库根运行（需要 assets/list/word.json），或设置 TYPEWORDS_ASSETS")?;
    let home = std::env::temp_dir().join(format!("tw-shot-home-{screen}"));
    let _ = std::fs::remove_dir_all(&home);
    let paths = Paths {
        home,
        assets: assets.clone(),
    };

    let mut app = app::build_for_test(paths, build_cli(&theme)).context("build app")?;
    if let Some(fatal) = &app.fatal {
        bail!("app reported fatal error: {fatal}");
    }
    prepare(&mut app, &screen)?;
    // 主题在导航之后再写一次：某些页面（如 settings）的按键操作可能临时改动它。
    app.boot.settings.theme = theme.clone();
    // 截图里不要出现启动/操作过程中的临时 toast，保持画面可复现。
    app.toasts.clear();

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).context("create terminal")?;
    terminal
        .draw(|frame| app.render(frame))
        .context("render app")?;
    let buffer = terminal.backend().buffer();

    if plain {
        print_plain(buffer, width, height);
    } else {
        let snapshot = json!({
            "width": width,
            "height": height,
            "screen": screen,
            "theme": theme,
            "rows": serialize_rows(buffer, width, height),
        });
        println!(
            "{}",
            serde_json::to_string(&snapshot).context("serialize snapshot")?
        );
    }
    Ok(())
}

fn parse_dimension(value: Option<&String>, default: u16, label: &str) -> Result<u16> {
    match value {
        None => Ok(default),
        Some(text) => {
            let parsed: u16 = text
                .parse()
                .with_context(|| format!("{label} must be a positive integer, got {text:?}"))?;
            if parsed == 0 || parsed > 1000 {
                bail!("{label} out of range (1..=1000): {parsed}");
            }
            Ok(parsed)
        }
    }
}

/// 通过 clap 解析参数构造 Cli：比结构体字面量更能容忍新增字段。
fn build_cli(theme: &str) -> Cli {
    // --seed 保证练习页内容可复现；--theme 走正式主题入口（避免启动回退提示）。
    match Cli::try_parse_from(["tw", "--theme", theme, "--seed", "7"]) {
        Ok(cli) => cli,
        Err(_) => Cli::parse_from(["tw"]),
    }
}

fn prepare(app: &mut App, screen: &str) -> Result<()> {
    match screen {
        "home" => app.screen = Screen::Home,
        "practice" => start_practice(app)?,
        "dict" => app.screen = Screen::DictPicker,
        "stats" => app.screen = Screen::Stats,
        "settings" => app.screen = Screen::Settings,
        "shortcuts" => app.screen = Screen::Shortcuts,
        "help" => app.screen = Screen::Help,
        "confirm" => {
            // 主页按 q 请求退出 → 确认弹层（不构造 ConfirmState）。
            app.screen = Screen::Home;
            press(app, KeyCode::Char('q'));
            if app.confirm.is_none() {
                bail!("pressing 'q' on home did not open the confirm overlay");
            }
        }
        "edit" => {
            // 设置页逐行 Enter，直到某个数字项打开编辑弹层。
            app.screen = Screen::Settings;
            open_edit_overlay(app)?;
        }
        "summary" => {
            start_practice(app)?;
            reach_summary(app)?;
        }
        other => bail!("unsupported screen {other:?}"),
    }
    Ok(())
}

fn start_practice(app: &mut App) -> Result<()> {
    app.screen = Screen::Home;
    press(app, KeyCode::Enter);
    if app.screen != Screen::Practice || app.session.is_none() {
        bail!(
            "could not start practice (screen={:?}, session={})",
            app.screen,
            app.session.is_some()
        );
    }
    Ok(())
}

fn open_edit_overlay(app: &mut App) -> Result<()> {
    for _ in 0..SETTING_ROWS.len().max(1) {
        press(app, KeyCode::Enter);
        if app.edit.is_some() {
            return Ok(());
        }
        if app.confirm.is_some() {
            press(app, KeyCode::Esc); // 取消 action 行触发的确认弹层
        }
        press(app, KeyCode::Down);
    }
    bail!("could not open the edit overlay by activating settings rows")
}

fn reach_summary(app: &mut App) -> Result<()> {
    // 练习页 Tab 跳过当前词；跳过整轮任务后 finish_session 会弹出结算页。
    for _ in 0..1000 {
        if app.summary.is_some() {
            return Ok(());
        }
        if app.session.is_none() {
            break;
        }
        press(app, KeyCode::Tab);
    }
    if app.summary.is_some() {
        Ok(())
    } else {
        bail!("could not reach the summary overlay by skipping words")
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn serialize_rows(buffer: &ratatui::buffer::Buffer, width: u16, height: u16) -> Vec<Value> {
    let mut rows = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut row = Vec::with_capacity(width as usize);
        for x in 0..width {
            row.push(match buffer.cell((x, y)) {
                Some(cell) => cell_json(cell.symbol(), cell.fg, cell.bg, cell.modifier),
                None => blank_cell_json(),
            });
        }
        rows.push(Value::Array(row));
    }
    rows
}

fn blank_cell_json() -> Value {
    json!({
        "sym": " ",
        "fg": "reset",
        "bg": "reset",
        "bold": false,
        "reversed": false,
        "underline": false,
        "width": 1,
    })
}

fn cell_json(symbol: &str, fg: Color, bg: Color, modifier: Modifier) -> Value {
    let display_width = UnicodeWidthStr::width(symbol).max(1);
    json!({
        "sym": symbol,
        "fg": serialize_color(fg),
        "bg": serialize_color(bg),
        "bold": modifier.contains(Modifier::BOLD),
        "reversed": modifier.contains(Modifier::REVERSED),
        "underline": modifier.contains(Modifier::UNDERLINED),
        "width": display_width,
    })
}

/// "reset" | 命名色 | "#rrggbb" | "indexed:N"
fn serialize_color(color: Color) -> String {
    match color {
        Color::Reset => "reset".to_string(),
        Color::Black => "black".to_string(),
        Color::Red => "red".to_string(),
        Color::Green => "green".to_string(),
        Color::Yellow => "yellow".to_string(),
        Color::Blue => "blue".to_string(),
        Color::Magenta => "magenta".to_string(),
        Color::Cyan => "cyan".to_string(),
        Color::Gray => "gray".to_string(),
        Color::DarkGray => "darkgray".to_string(),
        Color::LightRed => "lightred".to_string(),
        Color::LightGreen => "lightgreen".to_string(),
        Color::LightYellow => "lightyellow".to_string(),
        Color::LightBlue => "lightblue".to_string(),
        Color::LightMagenta => "lightmagenta".to_string(),
        Color::LightCyan => "lightcyan".to_string(),
        Color::White => "white".to_string(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(index) => format!("indexed:{index}"),
    }
}

fn print_plain(buffer: &ratatui::buffer::Buffer, width: u16, height: u16) {
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
                let symbol_width = UnicodeWidthStr::width(symbol);
                if symbol_width > 1 {
                    skip = symbol_width - 1;
                }
            }
        }
        println!("{line}");
    }
}

fn print_usage() {
    println!(
        "cargo run --quiet --example shot -- <screen> [width] [height] [theme] [--plain]\n\
         \n\
         screens: {}\n\
         width/height: terminal columns/rows (default 120x40)\n\
         theme: value written to app.boot.settings.theme (dark/light/high-contrast or custom name)\n\
         --plain: print monospace text instead of JSON\n\
         \n\
         JSON: width, height, screen, theme, rows. Each cell: sym, fg, bg, bold,\n\
         reversed, underline, width. Colors: reset | named | #rrggbb | indexed:N\n",
        SUPPORTED_SCREENS.join(", ")
    );
}
