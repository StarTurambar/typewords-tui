//! 系统 TTS：用系统自带的语音合成朗读文本。
//!
//! - Windows：所有朗读共用一个**常驻** PowerShell 会话，通过 stdin 逐行投喂
//!   `rate|text`，由 `System.Speech` 发声。整个运行期最多只起一个子进程：
//!   早期实现每句都新起一个 `powershell -WindowStyle Hidden`，既慢（每句加载
//!   一次 .NET），又会被安全软件的「隐藏执行 PowerShell」规则每句拦一次，
//!   多进程同时朗读还会抢麦。
//! - macOS：`say`。
//! - Linux：`espeak-ng`，退回 `spd-say`。这两个平台仍是一次朗读一个
//!   即发即忘的子进程。
//!
//! 所有路径都不阻塞界面：Windows 只往管道写一行，其余平台 `spawn` 后不等待。

#[cfg(not(windows))]
use std::process::{Command, Stdio};

/// 朗读一段文本。
///
/// `voice` 为空时用系统默认音色；`speed` 是 0.5~3 的倍速。
pub fn speak(text: &str, voice: &str, speed: f64) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    #[cfg(windows)]
    session::speak(text, voice, rate(speed));

    #[cfg(not(windows))]
    speak_once(text, voice, speed);
}

/// 0.5~3 的倍速 → `System.Speech` 的 -10~10 语速。
#[cfg(any(windows, test))]
fn rate(speed: f64) -> i32 {
    ((speed.clamp(0.5, 3.0) - 1.0) * 10.0)
        .round()
        .clamp(-10.0, 10.0) as i32
}

/// 投喂给常驻会话的一行：`rate|text`。
///
/// 换行一律压成空格——协议按行切分，一句话不能被拆成两条命令；只按**第一个**
/// `|` 切分，所以文本里的 `|` 与引号都不需要转义。旧实现把整句拼进
/// PowerShell 脚本再做单引号转义，既怕引号也怕换行。
#[cfg(any(windows, test))]
fn protocol_line(rate: i32, text: &str) -> String {
    let text = text.replace(['\r', '\n'], " ");
    format!("{rate}|{text}")
}

/// 常驻会话脚本：读 stdin、逐行朗读，stdin EOF 时结束。
///
/// 脚本保持纯 ASCII（命令行按本地代码页传递）；文本与音色都不拼进脚本文本：
/// 文本走 stdin，音色走 `$args[0]`。
#[cfg(any(windows, test))]
const SCRIPT: &str = "\
$ErrorActionPreference='Stop';
Add-Type -AssemblyName System.Speech;
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer;
$s.SetOutputToDefaultAudioDevice();
if ($args.Count -gt 0 -and $args[0]) { try { $s.SelectVoice($args[0]) } catch { } }
$reader = [System.IO.StreamReader]::new([Console]::OpenStandardInput(), [System.Text.Encoding]::UTF8);
while ($null -ne ($line = $reader.ReadLine())) {
  $bar = $line.IndexOf('|');
  if ($bar -lt 1) { continue }
  $rate = 0;
  if (-not [int]::TryParse($line.Substring(0, $bar), [ref]$rate)) { continue }
  $text = $line.Substring($bar + 1);
  if (-not $text) { continue }
  $s.Rate = [Math]::Max(-10, [Math]::Min(10, $rate));
  try { $s.SpeakAsyncCancelAll(); [void]$s.SpeakAsync($text) } catch { }
}
";

/// 非 Windows：一次朗读一个即发即忘的子进程。
#[cfg(not(windows))]
fn speak_once(text: &str, voice: &str, speed: f64) {
    let Some(mut command) = local_command(text, voice, speed) else {
        return;
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // 不 wait：子进程在后台继续，界面立刻恢复响应
    let _ = command.spawn();
}

/// 非 Windows 的朗读命令：macOS 用 `say`，Linux 用 `espeak-ng`。
#[cfg(not(windows))]
fn local_command(text: &str, voice: &str, speed: f64) -> Option<Command> {
    let speed = speed.clamp(0.5, 3.0);
    if cfg!(target_os = "macos") {
        let mut command = Command::new("say");
        if !voice.is_empty() {
            command.arg("-v").arg(voice);
        }
        // say 的语速是「字/分钟」，默认 175
        command.arg("-r").arg((175.0 * speed).round().to_string());
        command.arg(text);
        return Some(command);
    }
    if cfg!(unix) {
        let mut espeak = Command::new("espeak-ng");
        if !voice.is_empty() {
            espeak.arg("-v").arg(voice);
        }
        espeak.arg("-s").arg((175.0 * speed).round().to_string());
        espeak.arg(text);
        return Some(espeak);
    }
    None
}

/// 当前平台是否支持系统 TTS。
pub fn is_supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos", unix))
}

/// Windows 的常驻会话。
///
/// 为什么不能每句起一个进程：
/// - 进程创建本身会被安全软件「隐藏执行 PowerShell」规则拦截，每句一次弹窗；
/// - 每句都要重新加载 .NET 与 `System.Speech`（几百毫秒），朗读明显滞后；
/// - 多个 synthesizer 同时出声会互相抢音。
///
/// 因此这里只起一个进程、用 stdin 投喂，并在启动失败或发现进程已死时把
/// `UNAVAILABLE` 置位：本次运行不再重试，避免又变回「每句一次弹窗」。
#[cfg(windows)]
mod session {
    use std::io::{self, Write};
    use std::os::windows::process::CommandExt;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, MutexGuard};

    use super::{SCRIPT, protocol_line};

    /// `CreateProcess` 的「无窗口」标志：子进程不创建可见的控制台窗口。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub(super) struct Session {
        pub(super) child: Child,
        pub(super) stdin: ChildStdin,
        pub(super) voice: String,
    }

    /// 常驻会话。单线程应用里只有主线程会朗读，`Mutex` 只是给 `static` 一个
    /// 合法外壳（测试也可能并发调用）。
    static SESSION: Mutex<Option<Session>> = Mutex::new(None);

    /// 启动失败（例如被安全软件拦截）后，本次运行不再重试。
    static UNAVAILABLE: AtomicBool = AtomicBool::new(false);

    pub(super) fn speak(text: &str, voice: &str, rate: i32) {
        speak_with(&SESSION, &UNAVAILABLE, text, voice, rate, Session::start);
    }

    /// 朗读实现；`start` 可注入，测试用 `cmd /c more` 这种只读 stdin 的桩进程，
    /// 避免在 `cargo test` 里真的起 PowerShell。
    pub(super) fn speak_with<F>(
        slot: &Mutex<Option<Session>>,
        unavailable: &AtomicBool,
        text: &str,
        voice: &str,
        rate: i32,
        start: F,
    ) where
        F: Fn(&str) -> io::Result<Session>,
    {
        if unavailable.load(Ordering::Relaxed) {
            return;
        }
        let mut guard = lock(slot);
        let restart = match guard.as_mut() {
            None => true,
            Some(existing) => match existing.child.try_wait() {
                // 进程还活着：音色没变就复用，变了才重建。
                Ok(None) => existing.voice != voice,
                // 进程已经没了（多半是被安全软件拦掉）：放弃本次运行。
                Ok(Some(_)) | Err(_) => {
                    unavailable.store(true, Ordering::Relaxed);
                    *guard = None;
                    return;
                }
            },
        };
        if restart {
            match start(voice) {
                Ok(session) => *guard = Some(session),
                Err(_) => {
                    unavailable.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
        let Some(session) = guard.as_mut() else {
            return;
        };
        let mut line = protocol_line(rate, text);
        line.push('\n');
        if session.stdin.write_all(line.as_bytes()).is_err() {
            unavailable.store(true, Ordering::Relaxed);
            *guard = None;
        }
    }

    fn lock(slot: &Mutex<Option<Session>>) -> MutexGuard<'_, Option<Session>> {
        slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    impl Session {
        fn start(voice: &str) -> io::Result<Session> {
            Self::from_command(command(voice), voice)
        }

        /// 用给定命令起一个会话；命令必须能读 stdin 并逐行处理。
        pub(super) fn from_command(mut command: Command, voice: &str) -> io::Result<Session> {
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut child = command.spawn()?;
            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("TTS 子进程没有 stdin"))?;
            Ok(Session {
                child,
                stdin,
                voice: voice.to_string(),
            })
        }
    }

    /// 常驻会话的启动命令。
    pub(super) fn command(voice: &str) -> Command {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
        if !voice.is_empty() {
            command.arg(voice);
        }
        // 不要用 `-WindowStyle Hidden`：那正是安全软件「隐藏执行 PowerShell」
        // 规则最典型的匹配特征。CREATE_NO_WINDOW 同样不出窗口。
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_is_mapped_to_speech_rate() {
        assert_eq!(rate(1.0), 0);
        assert_eq!(rate(0.5), -5);
        assert_eq!(rate(0.1), -5, "低于 0.5 的倍速按 0.5 处理");
        assert_eq!(rate(3.0), 10);
        assert_eq!(rate(9.9), 10, "高于 3 的倍速按 3 处理");
    }

    #[test]
    fn protocol_line_keeps_one_line_per_sentence() {
        assert_eq!(protocol_line(0, "hello"), "0|hello");
        assert_eq!(
            protocol_line(-5, "line one\nline two"),
            "-5|line one line two",
            "换行必须压成空格，否则一句话会被拆成两条命令"
        );
        assert_eq!(protocol_line(3, "a|b"), "3|a|b", "文本里的竖线原样保留");
    }

    #[test]
    fn the_session_script_reads_utf8_lines_from_stdin() {
        assert!(SCRIPT.is_ascii(), "脚本必须纯 ASCII");
        assert!(SCRIPT.contains("OpenStandardInput"), "脚本应读 stdin");
        assert!(SCRIPT.contains("UTF8"), "stdin 必须按 UTF-8 解码");
        assert!(SCRIPT.contains("ReadLine"), "按行读取");
        assert!(
            SCRIPT.contains("SpeakAsyncCancelAll"),
            "新句子要打断上一句，避免朗读堆积"
        );
        assert!(SCRIPT.contains("$args[0]"), "音色走 argv，不拼进脚本");
    }

    #[cfg(not(windows))]
    #[test]
    fn say_platforms_build_a_per_call_command() {
        let command = local_command("hello", "Samantha", 1.0).expect("macOS / Linux 应能构造命令");
        let program = if cfg!(target_os = "macos") {
            "say"
        } else {
            "espeak-ng"
        };
        assert_eq!(command.get_program(), std::ffi::OsStr::new(program));
    }

    #[test]
    fn the_script_does_not_ask_for_a_hidden_window() {
        assert!(
            !SCRIPT.contains("WindowStyle"),
            "隐藏窗口是安全软件规则的匹配特征，改用 CREATE_NO_WINDOW"
        );
    }

    #[cfg(windows)]
    #[test]
    fn powershell_is_not_started_with_a_hidden_window() {
        let command = session::command("Microsoft Zira Desktop");
        assert_eq!(command.get_program(), std::ffi::OsStr::new("powershell"));
        let args: Vec<String> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|argument| argument == "-NoProfile"));
        assert!(args.iter().any(|argument| argument == "-NonInteractive"));
        assert!(args.iter().any(|argument| argument == "-Command"));
        assert!(args.iter().any(|argument| argument == SCRIPT));
        assert!(
            !args.iter().any(|argument| argument == "-WindowStyle"),
            "隐藏窗口是安全软件规则的匹配特征，改用 CREATE_NO_WINDOW"
        );
        assert!(
            args.iter()
                .any(|argument| argument == "Microsoft Zira Desktop"),
            "音色作为 argv 传入"
        );
    }

    #[cfg(windows)]
    fn stub_command() -> std::process::Command {
        // 只读 stdin、不输出的进程，充当常驻会话的替身。
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "more"]);
        command
    }

    #[cfg(windows)]
    #[test]
    fn one_process_serves_many_sentences() {
        use std::cell::Cell;
        use std::sync::Mutex;
        use std::sync::atomic::AtomicBool;

        let slot = Mutex::new(None);
        let unavailable = AtomicBool::new(false);
        let starts = Cell::new(0usize);
        let start = |voice: &str| {
            starts.set(starts.get() + 1);
            session::Session::from_command(stub_command(), voice)
        };
        session::speak_with(&slot, &unavailable, "first", "voice", 0, start);
        session::speak_with(&slot, &unavailable, "second", "voice", 0, start);
        assert_eq!(starts.get(), 1, "同一个音色只应启动一次进程");
        let mut guard = slot.lock().expect("测试内锁");
        let session = guard.as_mut().expect("会话应存在");
        assert!(
            session.child.try_wait().expect("try_wait").is_none(),
            "桩进程应还活着"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_voice_change_restarts_the_process() {
        use std::cell::Cell;
        use std::sync::Mutex;
        use std::sync::atomic::AtomicBool;

        let slot = Mutex::new(None);
        let unavailable = AtomicBool::new(false);
        let starts = Cell::new(0usize);
        let start = |voice: &str| {
            starts.set(starts.get() + 1);
            session::Session::from_command(stub_command(), voice)
        };
        session::speak_with(&slot, &unavailable, "first", "voice-a", 0, start);
        session::speak_with(&slot, &unavailable, "second", "voice-b", 0, start);
        assert_eq!(starts.get(), 2, "换音色需要重建会话");
    }
}
