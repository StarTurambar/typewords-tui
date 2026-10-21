//! 外部播放器：播放有道发音接口返回的 mp3。
//!
//! 只在设置里把发音来源切成「有道」时才会用到；下载与播放都交给系统命令，
//! 不引入任何 Rust 依赖：
//!
//! - 下载：`curl`（Windows 10+ 自带 curl.exe，macOS / Linux 也都有）。
//! - 播放：依次探测 `ffplay`、`mpv`、`vlc`。
//!
//! **不阻塞界面**：下载 + 播放整条链路由一个后台子进程完成，界面立刻恢复
//! 响应——早期实现在主线程上 `wait()` 等 curl 与播放器，最坏情况卡十几秒。
//! 能否降级回系统 TTS 由「工具是否装好了」决定（纯 PATH 查询，不起进程），
//! 判定在启动子进程之前完成。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 有道发音接口。
const PRONUNCIATION_API: &str = "https://dict.youdao.com/dictvoice?audio=";

/// 候选播放器，按优先级排列。
const PLAYERS: &[&str] = &["ffplay", "mpv", "vlc"];

/// 下载并播放单词发音。
///
/// `sound_type` 为 `"uk"` 时用英式（`type=1`），否则美式（`type=2`），
/// 与上游 `hooks/sound.ts` 一致。
///
/// 返回 `false` 表示「这条路径走不通」，调用方应降级回系统 TTS。判定依据是
/// 工具是否可用——真正下载失败时已经来不及降级（子进程在后台跑着）。
pub fn play_remote_pronunciation(word: &str, sound_type: &str) -> bool {
    if !is_ascii_word(word) {
        return false;
    }
    let kind = if sound_type == "uk" { 1 } else { 2 };
    let url = format!("{PRONUNCIATION_API}{word}&type={kind}");
    let Some(path) = audio_path(word, kind) else {
        return false;
    };
    // 工具不全就别白跑一趟：直接让调用方降级回系统 TTS。
    if !has_program("curl") || !PLAYERS.iter().any(|player| has_program(player)) {
        return false;
    }
    spawn_pipeline(&path, &url)
}

/// 是否是可以交给有道接口的单词（只含字母与连字符）。
fn is_ascii_word(word: &str) -> bool {
    !word.is_empty()
        && word.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '\''
        })
}

/// mp3 的存放路径（临时目录）。
fn audio_path(word: &str, kind: u8) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("typewords-tui-audio");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("{word}.{kind}.mp3")))
}

/// 程序是否在 PATH 里（只查文件系统，不起进程）。
fn has_program(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let candidates: Vec<String> = if cfg!(target_os = "windows") {
        vec![
            program.to_string(),
            format!("{program}.exe"),
            format!("{program}.cmd"),
            format!("{program}.bat"),
        ]
    } else {
        vec![program.to_string()]
    };
    std::env::split_paths(&path).any(|dir| candidates.iter().any(|name| dir.join(name).is_file()))
}

/// 后台跑「下载 → 播放 → 删除」这条链，界面不等它。
fn spawn_pipeline(path: &Path, url: &str) -> bool {
    let mut command = if cfg!(target_os = "windows") {
        let script = windows_script();
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ]);
        command.arg(path).arg(url);
        command
    } else {
        let script = unix_script();
        let mut command = Command::new("sh");
        command.args(["-c", &script, "sh"]);
        command.arg(path).arg(url);
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().is_ok()
}

/// Windows：PowerShell 里下载、依次试播放器、播完删文件。
///
/// 路径与 URL 走 `$args`，带空格也不会被拆开。
fn windows_script() -> String {
    let mut script = String::from(
        "$ErrorActionPreference='SilentlyContinue';\
         $p=$args[0]; $u=$args[1];\
         & curl.exe -sSL --max-time 10 -o $p $u;\
         if (-not (Test-Path -LiteralPath $p)) { exit 0 };\
         if (Get-Command ffplay -ErrorAction SilentlyContinue) { & ffplay -nodisp -autoexit -loglevel quiet $p }\
         elseif (Get-Command mpv -ErrorAction SilentlyContinue) { & mpv --no-video --really-quiet $p }\
         elseif (Get-Command vlc -ErrorAction SilentlyContinue) { & vlc --intf dummy --play-and-exit $p };\
         Remove-Item -LiteralPath $p -ErrorAction SilentlyContinue",
    );
    script.push('\n');
    script
}

/// macOS / Linux：sh 里下载、依次试播放器、播完删文件。
fn unix_script() -> String {
    let mut script = String::from(
        "p=$1; u=$2;\
         curl -sSL --max-time 10 -o \"$p\" \"$u\" || exit 0;\
         [ -f \"$p\" ] || exit 0;\
         if command -v ffplay >/dev/null 2>&1; then ffplay -nodisp -autoexit -loglevel quiet \"$p\";\
         elif command -v mpv >/dev/null 2>&1; then mpv --no-video --really-quiet \"$p\";\
         elif command -v vlc >/dev/null 2>&1; then vlc --intf dummy --play-and-exit \"$p\"; fi;\
         rm -f \"$p\"",
    );
    script.push('\n');
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_words_can_be_pronounced() {
        assert!(is_ascii_word("hello"));
        assert!(is_ascii_word("o'clock"));
        assert!(!is_ascii_word(""));
        assert!(!is_ascii_word("你好"));
        assert!(!is_ascii_word("a b"));
    }

    #[test]
    fn a_missing_program_is_reported_as_missing() {
        assert!(!has_program("definitely-not-a-real-program-xyz"));
    }

    #[test]
    fn the_scripts_take_the_path_and_url_as_arguments() {
        // 路径与 URL 不能被拼进脚本文本里——临时目录可能带空格。
        assert!(windows_script().contains("$args[0]"));
        assert!(unix_script().contains("$1"));
    }
}
