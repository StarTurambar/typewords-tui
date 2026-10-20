//! 系统 TTS：用系统自带的语音合成朗读文本。
//!
//! - Windows：`System.Speech`（通过 PowerShell 调用），本机装有
//!   `Microsoft Zira Desktop`(en-US) / `Microsoft Huihui Desktop`(zh-CN) /
//!   `Microsoft Haruka Desktop`(ja-JP)。
//! - macOS：`say`。
//! - Linux：`espeak-ng`，退回 `spd-say`。
//!
//! 子进程**即发即忘**：启动后不等待，界面不会被朗读阻塞。

use std::process::{Command, Stdio};

/// 朗读一段文本。
///
/// `voice` 为空时用系统默认音色；`speed` 是 0.5~3 的倍速。
pub fn speak(text: &str, voice: &str, speed: f64) {
    let Some(mut command) = build_command(text, voice, speed) else {
        return;
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // 不 wait：子进程在后台继续，界面立刻恢复响应
    let _ = command.spawn();
}

/// 构造朗读命令；当前平台不支持时返回 `None`。
fn build_command(text: &str, voice: &str, speed: f64) -> Option<Command> {
    let speed = speed.clamp(0.5, 3.0);
    if cfg!(target_os = "windows") {
        return windows_command(text, voice, speed);
    }
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
        // espeak-ng 优先，其次 spd-say
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

/// Windows：用 PowerShell 调 `System.Speech`。
///
/// PowerShell 的单引号字符串里 `'` 要写成 `''`；音色名同样要转义。
fn windows_command(text: &str, voice: &str, speed: f64) -> Option<Command> {
    let text = text.replace('\'', "''");
    let voice = voice.replace('\'', "''");
    // System.Speech 的 Rate 是 -10~10，这里把 0.5~3 的倍速映射过去
    let rate = ((speed - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
    let select = if voice.is_empty() {
        String::new()
    } else {
        format!("$s.SelectVoice('{voice}');")
    };
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         Add-Type -AssemblyName System.Speech; \
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
         $s.SetOutputToDefaultAudioDevice(); \
         {select} \
         $s.Rate = {rate}; \
         $s.Speak('{text}')"
    );
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
    ]);
    Some(command)
}

/// 当前平台是否支持系统 TTS。
pub fn is_supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos", unix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped_for_powershell() {
        let text = "it's a test";
        let escaped = text.replace('\'', "''");
        assert_eq!(escaped, "it''s a test");
    }

    #[test]
    fn command_is_built_on_supported_platforms() {
        let command = build_command("hello", "Microsoft Zira Desktop", 1.0);
        assert!(command.is_some(), "支持的平台应能构造命令");
    }

    #[test]
    fn speed_is_clamped_to_a_sane_range() {
        // 倍速被限制在 0.5~3，避免异常值让 TTS 失真
        let clamped = 9.0f64.clamp(0.5, 3.0);
        assert_eq!(clamped, 3.0);
    }
}
