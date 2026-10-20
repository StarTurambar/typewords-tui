//! 外部播放器：播放有道发音接口返回的 mp3。
//!
//! 只在设置里把发音来源切成「有道」时才会用到；下载与播放都交给系统命令，
//! 不引入任何 Rust 依赖：
//!
//! - 下载：`curl`（Windows 10+ 自带 curl.exe，macOS / Linux 也都有）。
//! - 播放：依次探测 `ffplay`、`mpv`、`vlc`。
//!
//! 任何一步失败都返回 `false`，由调用方降级回系统 TTS。

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// 有道发音接口。
const PRONUNCIATION_API: &str = "https://dict.youdao.com/dictvoice?audio=";

/// 候选播放器，按优先级排列。
const PLAYERS: &[&str] = &["ffplay", "mpv", "vlc"];

/// 下载并播放单词发音。
///
/// `sound_type` 为 `"uk"` 时用英式（`type=1`），否则美式（`type=2`），
/// 与上游 `hooks/sound.ts` 一致。
pub fn play_remote_pronunciation(word: &str, sound_type: &str) -> bool {
    if !is_ascii_word(word) {
        return false;
    }
    let Some(path) = download(word, sound_type) else {
        return false;
    };
    let played = play_file(&path);
    // 播放是异步的，文件先留着；下次下载时顺手清理旧的
    if played {
        spawn_cleanup(&path);
    } else {
        let _ = std::fs::remove_file(&path);
    }
    played
}

/// 是否是可以交给有道接口的单词（只含字母与连字符）。
fn is_ascii_word(word: &str) -> bool {
    !word.is_empty()
        && word.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '\''
        })
}

/// 下载 mp3 到临时目录。
fn download(word: &str, sound_type: &str) -> Option<PathBuf> {
    let kind = if sound_type == "uk" { 1 } else { 2 };
    let url = format!("{PRONUNCIATION_API}{word}&type={kind}");
    let dir = std::env::temp_dir().join("typewords-tui-audio");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{word}.{kind}.mp3"));
    let mut command = Command::new("curl");
    command
        .args(["-sSL", "--max-time", "10", "-o"])
        .arg(&path)
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = command.status().ok()?;
    if !status.success() || !path.exists() {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(path)
}

/// 用探测到的播放器播放文件。
fn play_file(path: &PathBuf) -> bool {
    for player in PLAYERS {
        let mut command = Command::new(player);
        match *player {
            "ffplay" => {
                command.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
            }
            "mpv" => {
                command.args(["--no-video", "--really-quiet"]);
            }
            "vlc" => {
                command.args(["--intf", "dummy", "--play-and-exit"]);
            }
            _ => {}
        }
        command
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Ok(status) = command.status()
            && status.success()
        {
            return true;
        }
    }
    false
}

/// 播放结束后删除临时文件。
fn spawn_cleanup(path: &std::path::Path) {
    let script = if cfg!(target_os = "windows") {
        format!(
            "Start-Sleep -Seconds 30; Remove-Item -LiteralPath '{}' -ErrorAction SilentlyContinue",
            path.display()
        )
    } else {
        format!("sleep 30; rm -f '{}'", path.display())
    };
    let mut command = Command::new(if cfg!(target_os = "windows") {
        "powershell"
    } else {
        "sh"
    });
    if cfg!(target_os = "windows") {
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    } else {
        command.args(["-c", &script]);
    }
    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
