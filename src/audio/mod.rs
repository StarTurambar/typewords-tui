//! 音频：终端响铃、系统 TTS 与外部播放器。
//!
//! 上游是浏览器应用，用 `<audio>` 播放 mp3、用 `speechSynthesis` 朗读。终端没有
//! 统一的音频通道，这里用三种可降级的方式对齐：
//!
//! - **键音 / 对错音** → 终端响铃（BEL）。零延迟、零资源，代价是听不出差别。
//! - **发音 / 朗读** → 系统自带的 TTS：Windows 用 `System.Speech`（PowerShell），
//!   macOS 用 `say`，Linux 用 `espeak-ng` / `spd-say`。
//! - **有道发音**（可选）→ 下载 mp3 后交给外部播放器（`ffplay` / `mpv` / `vlc`），
//!   失败自动降级回系统 TTS。
//!
//! 所有播放都是**即发即忘**的：子进程启动后不等待，界面不会被卡住。

pub mod bell;
pub mod player;
pub mod tts;

use crate::domain::types::SettingState;

/// 播放键音（终端响铃）。
pub fn play_key(settings: &SettingState) {
    if settings.keyboard_sound {
        bell::ring();
    }
}

/// 播放「答对」提示音。
pub fn play_correct(settings: &SettingState) {
    if settings.effect_sound {
        bell::ring();
    }
}

/// 播放「答错」提示音。
pub fn play_wrong(settings: &SettingState) {
    if settings.effect_sound {
        bell::ring();
    }
}

/// 朗读一段文本（单词或句子）。
///
/// `speed` 是 0.5~3 的倍速，映射到各平台的语速参数。
pub fn speak(settings: &SettingState, text: &str, speed: f64) {
    if text.trim().is_empty() {
        return;
    }
    match settings.tts_source.as_str() {
        "youdao" => {
            // 有道只提供单词发音；不是单词或下载失败时回退到系统 TTS
            if player::play_remote_pronunciation(text, settings.sound_type.as_str()) {
                return;
            }
            tts::speak(text, settings.tts_voice(&settings.sound_type), speed);
        }
        _ => tts::speak(text, settings.tts_voice(&settings.sound_type), speed),
    }
}

/// 朗读单词发音。
pub fn pronounce(settings: &SettingState, word: &str) {
    if !settings.word_sound {
        return;
    }
    speak(settings, word, settings.word_sound_speed);
}

/// 朗读文章句子。
pub fn speak_sentence(settings: &SettingState, sentence: &str) {
    if !settings.article_sound {
        return;
    }
    tts::speak(
        sentence,
        settings.tts_voice(&settings.sound_type),
        settings.article_sound_speed,
    );
}
