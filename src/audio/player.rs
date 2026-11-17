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
//!
//! **参数走环境变量，不走 `-Command` 之后的 argv**：Windows PowerShell 会把
//! `-Command` 之后的所有参数拼进命令文本**再解析一次**（`$args` 恒为空），
//! 而 URL 里的 `&` 是保留字符——拼进去必然 `ParserError`，脚本一行都不执行，
//! 调用方却只看到「进程起来了」，于是既不发音也不降级回系统 TTS。因此路径与 URL
//! 用 `TYPEWORDS_AUDIO_PATH` / `TYPEWORDS_AUDIO_URL` 传给脚本（`tts` 的音色参数
//! 踩过同一个坑）。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 有道发音接口。
const PRONUNCIATION_API: &str = "https://dict.youdao.com/dictvoice?audio=";

/// 候选播放器，按优先级排列。
const PLAYERS: &[&str] = &["ffplay", "mpv", "vlc"];

/// 后台脚本读取的两个环境变量名（见模块文档：不能改成 argv）。
const ENV_PATH: &str = "TYPEWORDS_AUDIO_PATH";
const ENV_URL: &str = "TYPEWORDS_AUDIO_URL";

/// 跑后台脚本的解释器。
const SHELL: &str = if cfg!(target_os = "windows") {
    "powershell"
} else {
    "sh"
};

/// `CreateProcess` 的「无窗口」标志：子进程不创建可见的控制台窗口。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
    // 工具不全就别白跑一趟：直接让调用方降级回系统 TTS。日志里留一行原因，
    // 否则用户切到有道后只会听到「系统 TTS」，不知道是缺 curl 还是缺播放器。
    if let Some(missing) = missing_tool() {
        crate::logging::warn(format!(
            "有道发音不可用：PATH 里找不到 {missing}，本次改用系统 TTS"
        ));
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

/// 脚本依赖但 PATH 里缺失的命令（全都在时为 `None`）。
///
/// 只查文件系统、不起进程；真正的下载失败发生在后台，那时已经来不及降级，
/// 所以「能不能跑」必须在这里一次性判定完。
fn missing_tool() -> Option<String> {
    if !has_program(SHELL) {
        return Some(SHELL.to_string());
    }
    if !has_program("curl") {
        return Some("curl".to_string());
    }
    if !PLAYERS.iter().any(|player| has_program(player)) {
        return Some(PLAYERS.join(" / "));
    }
    None
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
    match pipeline_command(path, url).spawn() {
        Ok(mut child) => {
            // 进程创建成功不等于脚本开始执行（解释器可能是个立刻退出的桩）。
            // 这里不等待——等待会卡住界面——只顺手收一次已经退出的子进程：
            // 真退了就说明这条链路没跑起来，交给调用方降级。
            !matches!(child.try_wait(), Ok(Some(_)))
        }
        Err(error) => {
            crate::logging::warn(format!("有道发音：启动下载 / 播放进程失败（{error}）"));
            false
        }
    }
}

/// 构造「下载 → 播放 → 删除」的命令。
///
/// 路径与 URL 走环境变量而不是 argv：见模块文档。
fn pipeline_command(path: &Path, url: &str) -> Command {
    let script = if cfg!(target_os = "windows") {
        windows_script()
    } else {
        unix_script()
    };
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-c", &script]);
        command
    };
    command
        .env(ENV_PATH, path)
        .env(ENV_URL, url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 不用 `-WindowStyle Hidden`：那正是安全软件「隐藏执行 PowerShell」
        // 规则最典型的匹配特征（同 `tts::command`）。CREATE_NO_WINDOW 同样不出窗口。
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Windows：PowerShell 里下载、依次试播放器、播完删文件。
///
/// 路径与 URL 从环境变量读（`$env:`），不读 `$args`：`-Command` 之后的
/// 参数会被 PowerShell 拼进命令文本，`$args` 永远是空的。
fn windows_script() -> String {
    let mut script = String::from(
        "$ErrorActionPreference='SilentlyContinue';\
         $p=$env:TYPEWORDS_AUDIO_PATH; $u=$env:TYPEWORDS_AUDIO_URL;\
         if (-not $p -or -not $u) { exit 1 };\
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
///
/// 与 Windows 用同一套环境变量契约，两个平台不各有一套参数传递方式。
fn unix_script() -> String {
    let mut script = String::from(
        "p=$TYPEWORDS_AUDIO_PATH; u=$TYPEWORDS_AUDIO_URL;\
         [ -n \"$p\" ] && [ -n \"$u\" ] || exit 1;\
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
    use std::ffi::{OsStr, OsString};

    /// 取命令里某个环境变量的值（没设置时为 `None`）。
    fn env_of(command: &Command, key: &str) -> Option<OsString> {
        command
            .get_envs()
            .find(|(name, _)| *name == OsStr::new(key))
            .and_then(|(_, value)| value.map(|value| value.to_os_string()))
    }

    fn args_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

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
    fn the_windows_script_reads_values_from_environment_not_positional_args() {
        // `-Command` 之后的 argv 会被 PowerShell 拼进命令文本再解析一次：
        // `$args` 永远是空的，URL 里的 `&` 还会直接 ParserError。
        let script = windows_script();
        assert!(script.contains("$env:TYPEWORDS_AUDIO_PATH"));
        assert!(script.contains("$env:TYPEWORDS_AUDIO_URL"));
        assert!(!script.contains("$args"), "路径与 URL 不能走位置参数");
        assert!(
            script.is_ascii(),
            "脚本必须纯 ASCII：命令行按本地代码页传递"
        );
    }

    #[test]
    fn the_unix_script_reads_values_from_the_same_environment() {
        let script = unix_script();
        assert!(script.contains("$TYPEWORDS_AUDIO_PATH"));
        assert!(script.contains("$TYPEWORDS_AUDIO_URL"));
        assert!(!script.contains("$1"), "位置参数不再使用");
        assert!(!script.contains("$2"), "位置参数不再使用");
    }

    #[test]
    fn the_pipeline_passes_the_path_and_url_through_the_environment() {
        // 临时目录可能带空格，URL 恒含 `&type=`：两个值都必须是环境变量，
        // 一旦落回 argv，Windows 上必然解析失败（静默不发音、也不降级）。
        let path = Path::new("C:/tmp/type words/hello.2.mp3");
        let url = "https://dict.youdao.com/dictvoice?audio=hello&type=2";
        let command = pipeline_command(path, url);
        let args = args_of(&command);
        assert!(
            !args.iter().any(|argument| argument.contains("hello.2.mp3")),
            "路径不能作为 argv：`-Command` 之后会被拼进脚本文本"
        );
        assert!(
            !args.iter().any(|argument| argument.contains("type=2")),
            "URL 不能作为 argv：其中的 `&` 会直接 ParserError"
        );
        assert_eq!(
            env_of(&command, ENV_PATH),
            Some(path.as_os_str().to_os_string())
        );
        assert_eq!(env_of(&command, ENV_URL), Some(OsString::from(url)));
    }

    #[test]
    fn the_pipeline_does_not_ask_for_a_hidden_window() {
        let args = args_of(&pipeline_command(Path::new("/tmp/a.mp3"), "https://e/x"));
        assert!(
            !args.iter().any(|argument| argument == "-WindowStyle"),
            "隐藏窗口是安全软件规则的匹配特征，改用 CREATE_NO_WINDOW"
        );
    }
}
