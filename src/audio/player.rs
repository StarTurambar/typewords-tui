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
use std::sync::OnceLock;

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

/// 脚本实际调用的 curl 名称。
///
/// Windows 的 PowerShell 把 `curl` 当成 `Invoke-WebRequest` 的别名，脚本因此
/// 固定调用 `curl.exe`；PATH 预检必须用同一个名字，否则只装了 `curl.cmd`
/// 的机器会「预检通过、脚本找不到命令」，既不发音也不降级。
const CURL: &str = if cfg!(target_os = "windows") {
    "curl.exe"
} else {
    "curl"
};

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
    // 工具不全就别白跑一趟：直接让调用方降级回系统 TTS。探测结果（以及那条
    // 「缺什么」的日志）在一次运行里只算一次，否则每个新词都会重扫 PATH 并往
    // 日志里追加一条相同的告警。
    if missing_tool().is_some() {
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
/// 所以「能不能跑」必须在这里一次性判定完。工具可用性在一次运行内是常量，
/// 探测结果缓存下来：每个新词都重扫 PATH 并重复写同一条日志没有意义。
fn missing_tool() -> Option<String> {
    static MISSING: OnceLock<Option<String>> = OnceLock::new();
    MISSING
        .get_or_init(|| {
            let missing = probe_missing_tool();
            if let Some(missing) = &missing {
                crate::logging::warn(format!(
                    "有道发音不可用：PATH 里找不到 {missing}，本次运行改用系统 TTS"
                ));
            }
            missing
        })
        .clone()
}

/// 实际探测：解释器、curl、任一播放器缺一不可。
fn probe_missing_tool() -> Option<String> {
    if !has_program(SHELL) {
        return Some(SHELL.to_string());
    }
    if !has_program(CURL) {
        return Some(CURL.to_string());
    }
    if !PLAYERS.iter().any(|player| has_program(player)) {
        return Some(PLAYERS.join(" / "));
    }
    None
}

/// 程序是否在 PATH 里且能执行（只查文件系统，不起进程）。
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
    std::env::split_paths(&path)
        .any(|dir| candidates.iter().any(|name| is_runnable(&dir.join(name))))
}

/// PATH 项是否是可执行文件。
///
/// Unix 上只检查 `is_file()` 会把没有执行位的同名文件当成可用工具：脚本里的
/// `command -v` 判定失败，一行都没跑，调用方却以为已交给后台、不再降级。
fn is_runnable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// 后台跑「下载 → 播放 → 删除」这条链，界面不等它。
fn spawn_pipeline(path: &Path, url: &str) -> bool {
    match pipeline_command(path, url).spawn() {
        Ok(mut child) => {
            // 进程创建成功不等于脚本开始执行（解释器可能是个立刻退出的桩）。
            // 这里不等待——等待会卡住界面——只顺手收一次已经退出的子进程：
            // 真退了就说明这条链路没跑起来，交给调用方降级。
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => {
                    crate::logging::warn(format!(
                        "有道发音：下载 / 播放进程启动后立即退出（{status}），本次改用系统 TTS"
                    ));
                    false
                }
                Err(error) => {
                    // 检查失败说明不了脚本没跑起来，维持原来的「已交给后台」判断。
                    crate::logging::warn(format!(
                        "有道发音：检查下载 / 播放进程状态失败（{error}）"
                    ));
                    true
                }
            }
        }
        Err(error) => {
            crate::logging::warn(format!("有道发音：启动下载 / 播放进程失败（{error}）"));
            false
        }
    }
}

/// 构造「下载 → 播放 → 删除」的命令。
///
/// 路径与 URL 走环境变量而不是 argv：见模块文档。解释器只用 [`SHELL`] 一处
/// 名字，预检与实际 spawn 不会各说各话。
fn pipeline_command(path: &Path, url: &str) -> Command {
    let script = if cfg!(target_os = "windows") {
        windows_script()
    } else {
        unix_script()
    };
    let mut command = Command::new(SHELL);
    if cfg!(target_os = "windows") {
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    } else {
        command.args(["-c", &script]);
    }
    command
        .env(ENV_PATH, path)
        .env(ENV_URL, url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    crate::audio::no_console_window(&mut command);
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

    /// PATH 预检必须检查脚本真正调用的那个 curl 名字。
    ///
    /// Windows 的 `curl` 在 PowerShell 里是 `Invoke-WebRequest` 别名，脚本
    /// 固定调 `curl.exe`；预检若仍查 `curl`，只装 `curl.cmd` 的机器会
    /// 「预检通过、脚本找不到命令」，静默不出声也不降级。
    #[test]
    fn the_path_precheck_uses_the_curl_name_the_script_runs() {
        let script = if cfg!(target_os = "windows") {
            windows_script()
        } else {
            unix_script()
        };
        assert!(
            script.contains(CURL),
            "预检名 {CURL} 必须与脚本实际调用的 curl 一致"
        );
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
