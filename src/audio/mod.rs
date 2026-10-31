//! 音频：终端响铃、系统 TTS 与外部播放器。
//!
//! 上游是浏览器应用，用 `<audio>` 播放 mp3、用 `speechSynthesis` 朗读。终端没有
//! 统一的音频通道，这里用三种可降级的方式对齐：
//!
//! - **键音 / 对错音** → 终端响铃（BEL）。零延迟、零资源，代价是听不出差别。
//! - **发音 / 朗读** → 系统自带的 TTS：Windows 用一个**常驻**的 PowerShell 会话
//!   调 `System.Speech`（见 `tts` 的模块文档），macOS 用 `say`，Linux 用
//!   `espeak-ng` / `spd-say`。
//! - **有道发音**（可选）→ 下载 mp3 后交给外部播放器（`ffplay` / `mpv` / `vlc`），
//!   失败自动降级回系统 TTS。
//!
//! 所有播放都不阻塞界面：Windows 的朗读只往常驻会话的 stdin 写一行，其余播放
//! 与外部命令都是**即发即忘**的子进程，启动后不等待。
//!
//! 朗读有两个调用点：界面上显式的「播放发音」（练习页 `Ctrl+P`）与练习推进时的
//! **自动朗读**（新词出现时，对应上游的「单词自动发音」开关）。两者都受
//! `word_sound` 控制，语义与上游 `TypeWord.playWord` 一致。

pub mod bell;
pub mod player;
pub mod tts;

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::types::SettingState;

/// 是否静音所有朗读（不含终端响铃）。
///
/// 只有测试会打开它：`App::build_for_test` 调 [`mute_speech`]，避免 `cargo test`
/// 真的启动系统 TTS 发声——练习页现在会在每次换词时自动朗读，不静音的话跑一遍
/// 测试就会念出几十个单词。正常运行时恒为 `false`。
static SPEECH_MUTED: AtomicBool = AtomicBool::new(false);

/// 静音朗读（仅供测试使用，正常运行时不要调用）。
#[doc(hidden)]
pub fn mute_speech() {
    SPEECH_MUTED.store(true, Ordering::Relaxed);
}

// 测试用的朗读记录。用 thread-local：`cargo test` 会并行跑多个用例，只有
// 打开记录的线程会写入，互不干扰。正常运行时 `RECORDING` 恒为 false。
thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static SPOKEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// 开始记录当前线程的朗读文本（仅供测试使用）。
#[doc(hidden)]
pub fn record_speech() {
    RECORDING.with(|recording| recording.set(true));
}

/// 取走当前线程已记录的朗读文本（仅供测试使用）。
#[doc(hidden)]
pub fn take_recorded_speech() -> Vec<String> {
    SPOKEN.with(|spoken| std::mem::take(&mut *spoken.borrow_mut()))
}

/// 朗读的唯一出口：测试记录 → 静音判断 → 系统 TTS。
fn deliver(text: &str, voice: &str, speed: f64) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    RECORDING.with(|recording| {
        if recording.get() {
            SPOKEN.with(|spoken| spoken.borrow_mut().push(text.to_string()));
        }
    });
    if SPEECH_MUTED.load(Ordering::Relaxed) {
        return;
    }
    tts::speak(text, voice, speed);
}

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
            deliver(text, settings.tts_voice(&settings.sound_type), speed);
        }
        _ => deliver(text, settings.tts_voice(&settings.sound_type), speed),
    }
}

/// 朗读单词发音（手动触发：练习页 `Ctrl+P`、单词测试页 `Ctrl+P`）。
///
/// 与自动朗读共用 `word_sound` 开关：关掉「单词发音」后既不自动读，也不响应
/// 手动键（与 [`crate::audio`] 文档里说明的语义一致）。
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
    deliver(
        sentence,
        settings.tts_voice(&settings.sound_type),
        settings.article_sound_speed,
    );
}
