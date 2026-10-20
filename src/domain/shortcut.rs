//! 快捷键表：数据化的「动作 → 键位」映射。
//!
//! 对齐上游 `app/core/types/enum.ts` 的 `ShortcutKey`（42 个动作）与
//! `app/core/config/env.ts` 的 `DefaultShortcutKeyMap`，键位字符串格式也���
//! 上游 `getShortcutKey` 一致（`Ctrl+`、`Alt+`、`Shift+` 前缀，方向键用
//! `➡⬅⬆⬇`，空格写作 `Space`）。
//!
//! 领域层不依赖终端 crate，因此基础键用本模块的 [`BaseKey`] 表示；
//! `app` 层负责把 `KeyEvent` 转成 [`ShortcutKey`]。

use std::collections::BTreeMap;
use std::fmt;

// ── 基础键 ─────────────────────────────────────────────────────────────────

/// 基础键（与终端事件解耦，便于序列化与比较）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BaseKey {
    /// 普通字符（字母统一大写，与上游 `e.key.toUpperCase()` 一致）
    Char(char),
    Space,
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// 功能键 F1..F12
    F(u8),
}

impl BaseKey {
    /// 上游写法：字母大写，方向键用箭头符号，空格写作 `Space`。
    pub fn to_upstream(self) -> String {
        match self {
            Self::Char(character) => character.to_uppercase().to_string(),
            Self::Space => "Space".to_string(),
            Self::Enter => "Enter".to_string(),
            Self::Esc => "Escape".to_string(),
            Self::Tab => "Tab".to_string(),
            Self::Backspace => "Backspace".to_string(),
            Self::Delete => "Delete".to_string(),
            Self::Insert => "Insert".to_string(),
            Self::Left => "⬅".to_string(),
            Self::Right => "➡".to_string(),
            Self::Up => "⬆".to_string(),
            Self::Down => "⬇".to_string(),
            Self::Home => "Home".to_string(),
            Self::End => "End".to_string(),
            Self::PageUp => "PageUp".to_string(),
            Self::PageDown => "PageDown".to_string(),
            Self::F(number) => format!("F{number}"),
        }
    }

    /// 解析上游写法；无法识别时返回 `None`。
    pub fn from_upstream(text: &str) -> Option<Self> {
        if text.chars().count() == 1 {
            let character = text.chars().next()?;
            return Some(Self::Char(character.to_ascii_uppercase()));
        }
        Some(match text {
            "Space" | " " => Self::Space,
            "Enter" => Self::Enter,
            "Escape" | "Esc" => Self::Esc,
            "Tab" => Self::Tab,
            "Backspace" => Self::Backspace,
            "Delete" => Self::Delete,
            "Insert" => Self::Insert,
            "➡" | "ArrowRight" | "Right" => Self::Right,
            "⬅" | "ArrowLeft" | "Left" => Self::Left,
            "⬆" | "ArrowUp" | "Up" => Self::Up,
            "⬇" | "ArrowDown" | "Down" => Self::Down,
            "Home" => Self::Home,
            "End" => Self::End,
            "PageUp" => Self::PageUp,
            "PageDown" => Self::PageDown,
            other => {
                let number = other.strip_prefix('F')?.parse::<u8>().ok()?;
                if !(1..=12).contains(&number) {
                    return None;
                }
                Self::F(number)
            }
        })
    }
}

// ── 键位 ───────────────────────────────────────────────────────────────────

/// 一个可绑定的键位：修饰键 + 基础键。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShortcutKey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: BaseKey,
}

impl ShortcutKey {
    pub fn new(key: BaseKey) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            key,
        }
    }

    pub fn ctrl(key: BaseKey) -> Self {
        Self {
            ctrl: true,
            ..Self::new(key)
        }
    }

    pub fn alt(key: BaseKey) -> Self {
        Self {
            alt: true,
            ..Self::new(key)
        }
    }

    pub fn shift(key: BaseKey) -> Self {
        Self {
            shift: true,
            ..Self::new(key)
        }
    }

    /// 上游写法：`Ctrl+` / `Alt+` / `Shift+` 前缀 + 基础键。
    pub fn to_upstream(self) -> String {
        let mut text = String::new();
        if self.ctrl {
            text.push_str("Ctrl+");
        }
        if self.alt {
            text.push_str("Alt+");
        }
        if self.shift {
            text.push_str("Shift+");
        }
        text.push_str(&self.key.to_upstream());
        text
    }

    /// 解析上游写法；无法识别时返回 `None`。
    pub fn from_upstream(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut rest = text;
        loop {
            if let Some(tail) = rest.strip_prefix("Ctrl+") {
                ctrl = true;
                rest = tail;
            } else if let Some(tail) = rest.strip_prefix("Alt+") {
                alt = true;
                rest = tail;
            } else if let Some(tail) = rest.strip_prefix("Shift+") {
                shift = true;
                rest = tail;
            } else {
                break;
            }
        }
        Some(Self {
            ctrl,
            alt,
            shift,
            key: BaseKey::from_upstream(rest)?,
        })
    }
}

impl fmt::Display for ShortcutKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_upstream())
    }
}

// ── 动作 ───────────────────────────────────────────────────────────────────

/// 可配置的动作，对齐上游 `ShortcutKey`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShortcutAction {
    EditArticle,
    PlayWordPronunciation,
    ToggleShowTranslate,
    ToggleDictation,
    ToggleTheme,
    ToggleConciseMode,
    ToggleToolbar,
    TogglePanel,
    RandomWrite,
    Previous,
    Next,
    PreviousChapter,
    NextChapter,
    ShowWord,
    Ignore,
    ToggleSimple,
    ToggleCollect,
    CollectToDict,
    NextStep,
    RepeatChapter,
    DictationChapter,
    KnowWord,
    UnknownWord,
    MasteredWord,
    ChooseA,
    ChooseB,
    ChooseC,
    ChooseD,
    WordTestingNext,
    SelfTestingChooseA,
    SelfTestingChooseB,
    SelfTestingChooseC,
    SelfTestingChooseD,
    PlaySentence1,
    PlaySentence2,
    PlaySentence3,
    PlaySentence4,
    PlaySentence5,
    PlaySentence6,
    PlaySentence7,
    PlaySentence8,
    PlaySentence9,
}

impl ShortcutAction {
    /// 全部动作，顺序与上游枚举一致。
    pub const ALL: [ShortcutAction; 42] = [
        ShortcutAction::EditArticle,
        ShortcutAction::PlayWordPronunciation,
        ShortcutAction::ToggleShowTranslate,
        ShortcutAction::ToggleDictation,
        ShortcutAction::ToggleTheme,
        ShortcutAction::ToggleConciseMode,
        ShortcutAction::ToggleToolbar,
        ShortcutAction::TogglePanel,
        ShortcutAction::RandomWrite,
        ShortcutAction::Previous,
        ShortcutAction::Next,
        ShortcutAction::PreviousChapter,
        ShortcutAction::NextChapter,
        ShortcutAction::ShowWord,
        ShortcutAction::Ignore,
        ShortcutAction::ToggleSimple,
        ShortcutAction::ToggleCollect,
        ShortcutAction::CollectToDict,
        ShortcutAction::NextStep,
        ShortcutAction::RepeatChapter,
        ShortcutAction::DictationChapter,
        ShortcutAction::KnowWord,
        ShortcutAction::UnknownWord,
        ShortcutAction::MasteredWord,
        ShortcutAction::ChooseA,
        ShortcutAction::ChooseB,
        ShortcutAction::ChooseC,
        ShortcutAction::ChooseD,
        ShortcutAction::WordTestingNext,
        ShortcutAction::SelfTestingChooseA,
        ShortcutAction::SelfTestingChooseB,
        ShortcutAction::SelfTestingChooseC,
        ShortcutAction::SelfTestingChooseD,
        ShortcutAction::PlaySentence1,
        ShortcutAction::PlaySentence2,
        ShortcutAction::PlaySentence3,
        ShortcutAction::PlaySentence4,
        ShortcutAction::PlaySentence5,
        ShortcutAction::PlaySentence6,
        ShortcutAction::PlaySentence7,
        ShortcutAction::PlaySentence8,
        ShortcutAction::PlaySentence9,
    ];

    /// 上游 `ShortcutKey` 的枚举名，存档 `shortcutKeyMap` 的 key。
    pub fn upstream_name(self) -> &'static str {
        match self {
            ShortcutAction::EditArticle => "EditArticle",
            ShortcutAction::PlayWordPronunciation => "PlayWordPronunciation",
            ShortcutAction::ToggleShowTranslate => "ToggleShowTranslate",
            ShortcutAction::ToggleDictation => "ToggleDictation",
            ShortcutAction::ToggleTheme => "ToggleTheme",
            ShortcutAction::ToggleConciseMode => "ToggleConciseMode",
            ShortcutAction::ToggleToolbar => "ToggleToolbar",
            ShortcutAction::TogglePanel => "TogglePanel",
            ShortcutAction::RandomWrite => "RandomWrite",
            ShortcutAction::Previous => "Previous",
            ShortcutAction::Next => "Next",
            ShortcutAction::PreviousChapter => "PreviousChapter",
            ShortcutAction::NextChapter => "NextChapter",
            ShortcutAction::ShowWord => "ShowWord",
            ShortcutAction::Ignore => "Ignore",
            ShortcutAction::ToggleSimple => "ToggleSimple",
            ShortcutAction::ToggleCollect => "ToggleCollect",
            ShortcutAction::CollectToDict => "CollectToDict",
            ShortcutAction::NextStep => "NextStep",
            ShortcutAction::RepeatChapter => "RepeatChapter",
            ShortcutAction::DictationChapter => "DictationChapter",
            ShortcutAction::KnowWord => "KnowWord",
            ShortcutAction::UnknownWord => "UnknownWord",
            ShortcutAction::MasteredWord => "MasteredWord",
            ShortcutAction::ChooseA => "ChooseA",
            ShortcutAction::ChooseB => "ChooseB",
            ShortcutAction::ChooseC => "ChooseC",
            ShortcutAction::ChooseD => "ChooseD",
            ShortcutAction::WordTestingNext => "WordTestingNext",
            ShortcutAction::SelfTestingChooseA => "SelfTestingChooseA",
            ShortcutAction::SelfTestingChooseB => "SelfTestingChooseB",
            ShortcutAction::SelfTestingChooseC => "SelfTestingChooseC",
            ShortcutAction::SelfTestingChooseD => "SelfTestingChooseD",
            ShortcutAction::PlaySentence1 => "PlaySentence1",
            ShortcutAction::PlaySentence2 => "PlaySentence2",
            ShortcutAction::PlaySentence3 => "PlaySentence3",
            ShortcutAction::PlaySentence4 => "PlaySentence4",
            ShortcutAction::PlaySentence5 => "PlaySentence5",
            ShortcutAction::PlaySentence6 => "PlaySentence6",
            ShortcutAction::PlaySentence7 => "PlaySentence7",
            ShortcutAction::PlaySentence8 => "PlaySentence8",
            ShortcutAction::PlaySentence9 => "PlaySentence9",
        }
    }

    /// 由上游枚举名反查动作。
    pub fn from_upstream_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.upstream_name() == name)
    }

    /// 设置页显示名。
    pub fn label(self) -> &'static str {
        match self {
            ShortcutAction::EditArticle => "编辑文章",
            ShortcutAction::PlayWordPronunciation => "播放单词发音",
            ShortcutAction::ToggleShowTranslate => "切换翻译",
            ShortcutAction::ToggleDictation => "切换默写遮罩",
            ShortcutAction::ToggleTheme => "切换主题",
            ShortcutAction::ToggleConciseMode => "切换简洁模式",
            ShortcutAction::ToggleToolbar => "切换工具栏",
            ShortcutAction::TogglePanel => "切换词表面板",
            ShortcutAction::RandomWrite => "随机默写",
            ShortcutAction::Previous => "上一个词",
            ShortcutAction::Next => "下一个词",
            ShortcutAction::PreviousChapter => "上一章",
            ShortcutAction::NextChapter => "下一章",
            ShortcutAction::ShowWord => "显示单词",
            ShortcutAction::Ignore => "跳过单词",
            ShortcutAction::ToggleSimple => "标记已掌握",
            ShortcutAction::ToggleCollect => "收藏",
            ShortcutAction::CollectToDict => "收藏到指定词典",
            ShortcutAction::NextStep => "跳到下一阶段",
            ShortcutAction::RepeatChapter => "重复本章",
            ShortcutAction::DictationChapter => "默写本章",
            ShortcutAction::KnowWord => "自测：认识",
            ShortcutAction::UnknownWord => "自测：不认识",
            ShortcutAction::MasteredWord => "自测：已掌握",
            ShortcutAction::ChooseA => "选择 A",
            ShortcutAction::ChooseB => "选择 B",
            ShortcutAction::ChooseC => "选择 C",
            ShortcutAction::ChooseD => "选择 D",
            ShortcutAction::WordTestingNext => "单词测试：下一题",
            ShortcutAction::SelfTestingChooseA => "自测选择 A",
            ShortcutAction::SelfTestingChooseB => "自测选择 B",
            ShortcutAction::SelfTestingChooseC => "自测选择 C",
            ShortcutAction::SelfTestingChooseD => "自测选择 D",
            ShortcutAction::PlaySentence1 => "播放例句 1",
            ShortcutAction::PlaySentence2 => "播放例句 2",
            ShortcutAction::PlaySentence3 => "播放例句 3",
            ShortcutAction::PlaySentence4 => "播放例句 4",
            ShortcutAction::PlaySentence5 => "播放例句 5",
            ShortcutAction::PlaySentence6 => "播放例句 6",
            ShortcutAction::PlaySentence7 => "播放例句 7",
            ShortcutAction::PlaySentence8 => "播放例句 8",
            ShortcutAction::PlaySentence9 => "播放例句 9",
        }
    }

    /// 该动作是否在当前版本有实际功能。
    ///
    /// 上游的部分动作依赖音频或文章编辑，本项目尚未实现对应能力时
    /// 仍保留绑定（保持存档互通），但设置页标注为「暂不可用」。
    pub fn is_available(self) -> bool {
        !matches!(
            self,
            ShortcutAction::EditArticle
                | ShortcutAction::PlayWordPronunciation
                | ShortcutAction::PlaySentence1
                | ShortcutAction::PlaySentence2
                | ShortcutAction::PlaySentence3
                | ShortcutAction::PlaySentence4
                | ShortcutAction::PlaySentence5
                | ShortcutAction::PlaySentence6
                | ShortcutAction::PlaySentence7
                | ShortcutAction::PlaySentence8
                | ShortcutAction::PlaySentence9
        )
    }
}

// ── 默认绑定 ───────────────────────────────────────────────────────────────

/// 上游 `DefaultShortcutKeyMap` 的默认值。
///
/// 与上游的差异（终端限制）：
/// - `ToggleConciseMode` 上游是 `Ctrl+M`，在多数终端等同回车，改用 `Ctrl+G`。
/// - `PlayWordPronunciation` 上游是 `Ctrl+P`，本项目暂未实现发音，保持该绑定
///   以便与上游存档一致，暂停计时改用 TUI 专有的 `Ctrl+,`。
pub fn default_bindings() -> BTreeMap<ShortcutAction, ShortcutKey> {
    use BaseKey::*;
    use ShortcutAction::*;
    // 字母统一大写：上游 `getShortcutKey` 对 A-Z 一律转大写
    let letter = |character: char| Char(character.to_ascii_uppercase());
    let pairs: Vec<(ShortcutAction, ShortcutKey)> = vec![
        (EditArticle, ShortcutKey::ctrl(letter('e'))),
        (PlayWordPronunciation, ShortcutKey::ctrl(letter('p'))),
        (ToggleShowTranslate, ShortcutKey::ctrl(letter('z'))),
        (ToggleDictation, ShortcutKey::ctrl(letter('i'))),
        (ToggleTheme, ShortcutKey::ctrl(letter('q'))),
        (ToggleConciseMode, ShortcutKey::ctrl(letter('g'))),
        (ToggleToolbar, ShortcutKey::ctrl(letter('b'))),
        (TogglePanel, ShortcutKey::ctrl(letter('l'))),
        (RandomWrite, ShortcutKey::ctrl(letter('r'))),
        (Previous, ShortcutKey::ctrl(Left)),
        (Next, ShortcutKey::ctrl(Right)),
        (PreviousChapter, ShortcutKey::alt(Left)),
        (NextChapter, ShortcutKey::alt(Right)),
        (ShowWord, ShortcutKey::new(Esc)),
        (Ignore, ShortcutKey::new(Tab)),
        (ToggleSimple, ShortcutKey::new(Char('`'))),
        (ToggleCollect, ShortcutKey::new(Enter)),
        (CollectToDict, ShortcutKey::shift(Enter)),
        (NextStep, ShortcutKey::shift(Right)),
        (RepeatChapter, ShortcutKey::ctrl(Enter)),
        (DictationChapter, ShortcutKey::alt(Enter)),
        (KnowWord, ShortcutKey::new(Char('1'))),
        (UnknownWord, ShortcutKey::new(Char('2'))),
        (MasteredWord, ShortcutKey::new(Char('3'))),
        (ChooseA, ShortcutKey::new(Char('1'))),
        (ChooseB, ShortcutKey::new(Char('2'))),
        (ChooseC, ShortcutKey::new(Char('3'))),
        (ChooseD, ShortcutKey::new(Char('4'))),
        (WordTestingNext, ShortcutKey::new(Space)),
        (SelfTestingChooseA, ShortcutKey::alt(Char('1'))),
        (SelfTestingChooseB, ShortcutKey::alt(Char('2'))),
        (SelfTestingChooseC, ShortcutKey::alt(Char('3'))),
        (SelfTestingChooseD, ShortcutKey::alt(Char('4'))),
        (PlaySentence1, ShortcutKey::ctrl(Char('1'))),
        (PlaySentence2, ShortcutKey::ctrl(Char('2'))),
        (PlaySentence3, ShortcutKey::ctrl(Char('3'))),
        (PlaySentence4, ShortcutKey::ctrl(Char('4'))),
        (PlaySentence5, ShortcutKey::ctrl(Char('5'))),
        (PlaySentence6, ShortcutKey::ctrl(Char('6'))),
        (PlaySentence7, ShortcutKey::ctrl(Char('7'))),
        (PlaySentence8, ShortcutKey::ctrl(Char('8'))),
        (PlaySentence9, ShortcutKey::ctrl(Char('9'))),
    ];
    pairs.into_iter().collect()
}

/// 终端兼容别名：不落盘，只在默认绑定之外补充。
///
/// 部分终端会吞掉 `Ctrl+方向键`，另一些把 `Ctrl+J` 报成换行；这些 ASCII
/// 别名保证核心操作在任何终端都可达。
pub fn alias_bindings() -> BTreeMap<ShortcutAction, Vec<ShortcutKey>> {
    use BaseKey::*;
    use ShortcutAction::*;
    let letter = |character: char| Char(character.to_ascii_uppercase());
    let pairs: Vec<(ShortcutAction, Vec<ShortcutKey>)> = vec![
        (Next, vec![ShortcutKey::ctrl(letter('n'))]),
        (NextStep, vec![ShortcutKey::ctrl(letter('j'))]),
    ];
    pairs.into_iter().collect()
}

// ── 快捷键表 ───────────────────────────────────────────────────────────────

/// 快捷键表：动作 → 键位，并提供反向查找与冲突检测。
#[derive(Clone, Debug)]
pub struct ShortcutMap {
    bindings: BTreeMap<ShortcutAction, ShortcutKey>,
}

impl Default for ShortcutMap {
    /// 默认即上游的默认绑定表。
    fn default() -> Self {
        Self {
            bindings: default_bindings(),
        }
    }
}

impl ShortcutMap {
    /// 由存档读入；缺失或非法的条目回退到默认绑定。
    pub fn from_stored(stored: &BTreeMap<String, String>) -> Self {
        let defaults = default_bindings();
        let mut bindings = defaults;
        for (name, text) in stored {
            let Some(action) = ShortcutAction::from_upstream_name(name) else {
                continue;
            };
            let Some(key) = ShortcutKey::from_upstream(text) else {
                continue;
            };
            bindings.insert(action, key);
        }
        Self { bindings }
    }

    /// 导出为存档格式（上游 `shortcutKeyMap`）。
    pub fn to_stored(&self) -> BTreeMap<String, String> {
        self.bindings
            .iter()
            .map(|(action, key)| (action.upstream_name().to_string(), key.to_upstream()))
            .collect()
    }

    /// 动作的主键位。
    pub fn key_for(&self, action: ShortcutAction) -> ShortcutKey {
        self.bindings
            .get(&action)
            .copied()
            .or_else(|| default_bindings().get(&action).copied())
            .unwrap_or_else(|| ShortcutKey::new(BaseKey::Esc))
    }

    /// 反向查找：命中该键位的所有动作。
    ///
    /// 同一键位可以绑定多个动作（例如「1」既是自测的「认识」也是选择题的 A），
    /// 具体取哪一个由调用方按当前场景决定。终端兼容别名同样参与匹配。
    pub fn actions_for(&self, key: &ShortcutKey) -> Vec<ShortcutAction> {
        let aliases = alias_bindings();
        let mut result: Vec<ShortcutAction> = Vec::new();
        for (action, bound) in &self.bindings {
            if bound == key && !result.contains(action) {
                result.push(*action);
            }
        }
        for (action, extra) in &aliases {
            if extra.contains(key) && !result.contains(action) {
                result.push(*action);
            }
        }
        result
    }

    /// 改绑；返回 `Err` 表示该键位已被别的动作占用。
    pub fn set(&mut self, action: ShortcutAction, key: ShortcutKey) -> Result<(), String> {
        let clash = self
            .bindings
            .iter()
            .find(|(other, bound)| **other != action && **bound == key)
            .map(|(other, _)| *other);
        if let Some(other) = clash {
            return Err(format!("该键位已被「{}」占用", other.label()));
        }
        self.bindings.insert(action, key);
        Ok(())
    }

    /// 恢复默认绑定。
    pub fn reset(&mut self, action: ShortcutAction) {
        if let Some(key) = default_bindings().get(&action).copied() {
            self.bindings.insert(action, key);
        }
    }

    /// 全部恢复默认。
    pub fn reset_all(&mut self) {
        self.bindings = default_bindings();
    }

    /// 当前所有冲突（同键绑定到多个动作）。
    pub fn conflicts(&self) -> Vec<(ShortcutAction, ShortcutAction)> {
        let mut seen: BTreeMap<ShortcutKey, Vec<ShortcutAction>> = BTreeMap::new();
        for (action, key) in &self.bindings {
            seen.entry(*key).or_default().push(*action);
        }
        let mut result = Vec::new();
        for actions in seen.values() {
            for (index, action) in actions.iter().enumerate() {
                for other in &actions[index + 1..] {
                    result.push((*action, *other));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_key_round_trips() {
        for text in [
            "Ctrl+➡",
            "Alt+⬅",
            "Shift+Enter",
            "Escape",
            "Space",
            "Tab",
            "Enter",
            "`",
            "1",
            "Alt+1",
            "Ctrl+1",
        ] {
            let key = ShortcutKey::from_upstream(text).expect(text);
            assert_eq!(key.to_upstream(), text);
        }
    }

    #[test]
    fn default_bindings_cover_every_action() {
        let defaults = default_bindings();
        assert_eq!(defaults.len(), ShortcutAction::ALL.len());
        for action in ShortcutAction::ALL {
            assert!(defaults.contains_key(&action), "{action:?} 缺少默认绑定");
        }
    }

    #[test]
    fn upstream_default_map_is_matched() {
        // 上游 env.ts 的 DefaultShortcutKeyMap 逐条核对
        let expected: Vec<(ShortcutAction, &str)> = vec![
            (ShortcutAction::EditArticle, "Ctrl+E"),
            (ShortcutAction::PlayWordPronunciation, "Ctrl+P"),
            (ShortcutAction::ToggleShowTranslate, "Ctrl+Z"),
            (ShortcutAction::ToggleDictation, "Ctrl+I"),
            (ShortcutAction::ToggleTheme, "Ctrl+Q"),
            (ShortcutAction::ToggleConciseMode, "Ctrl+G"),
            (ShortcutAction::ToggleToolbar, "Ctrl+B"),
            (ShortcutAction::TogglePanel, "Ctrl+L"),
            (ShortcutAction::RandomWrite, "Ctrl+R"),
            (ShortcutAction::Previous, "Ctrl+⬅"),
            (ShortcutAction::Next, "Ctrl+➡"),
            (ShortcutAction::PreviousChapter, "Alt+⬅"),
            (ShortcutAction::NextChapter, "Alt+➡"),
            (ShortcutAction::ShowWord, "Escape"),
            (ShortcutAction::Ignore, "Tab"),
            (ShortcutAction::ToggleSimple, "`"),
            (ShortcutAction::ToggleCollect, "Enter"),
            (ShortcutAction::CollectToDict, "Shift+Enter"),
            (ShortcutAction::NextStep, "Shift+➡"),
            (ShortcutAction::RepeatChapter, "Ctrl+Enter"),
            (ShortcutAction::DictationChapter, "Alt+Enter"),
            (ShortcutAction::KnowWord, "1"),
            (ShortcutAction::UnknownWord, "2"),
            (ShortcutAction::MasteredWord, "3"),
            (ShortcutAction::ChooseA, "1"),
            (ShortcutAction::ChooseB, "2"),
            (ShortcutAction::ChooseC, "3"),
            (ShortcutAction::ChooseD, "4"),
            (ShortcutAction::WordTestingNext, "Space"),
            (ShortcutAction::SelfTestingChooseA, "Alt+1"),
            (ShortcutAction::SelfTestingChooseB, "Alt+2"),
            (ShortcutAction::SelfTestingChooseC, "Alt+3"),
            (ShortcutAction::SelfTestingChooseD, "Alt+4"),
            (ShortcutAction::PlaySentence1, "Ctrl+1"),
            (ShortcutAction::PlaySentence2, "Ctrl+2"),
            (ShortcutAction::PlaySentence3, "Ctrl+3"),
            (ShortcutAction::PlaySentence4, "Ctrl+4"),
            (ShortcutAction::PlaySentence5, "Ctrl+5"),
            (ShortcutAction::PlaySentence6, "Ctrl+6"),
            (ShortcutAction::PlaySentence7, "Ctrl+7"),
            (ShortcutAction::PlaySentence8, "Ctrl+8"),
            (ShortcutAction::PlaySentence9, "Ctrl+9"),
        ];
        let defaults = default_bindings();
        for (action, text) in expected {
            assert_eq!(
                defaults[&action].to_upstream(),
                text,
                "{action:?} 的默认绑定应与上游一致"
            );
        }
    }

    #[test]
    fn stored_map_falls_back_to_defaults() {
        let mut stored = BTreeMap::new();
        stored.insert("ShowWord".to_string(), "Ctrl+K".to_string());
        stored.insert("NotAnAction".to_string(), "Ctrl+Y".to_string());
        stored.insert("Ignore".to_string(), "!!!".to_string());
        let map = ShortcutMap::from_stored(&stored);
        assert_eq!(
            map.key_for(ShortcutAction::ShowWord).to_upstream(),
            "Ctrl+K"
        );
        // 非法条目回退默认
        assert_eq!(map.key_for(ShortcutAction::Ignore).to_upstream(), "Tab");
        // 未知动作被忽略
        assert_eq!(map.to_stored().len(), ShortcutAction::ALL.len());
    }

    #[test]
    fn setting_a_conflicting_key_is_rejected() {
        let mut map = ShortcutMap::default();
        let key = map.key_for(ShortcutAction::ToggleCollect);
        let error = map
            .set(ShortcutAction::ShowWord, key)
            .expect_err("同一键位不应被两个动作占用");
        assert!(error.contains("收藏"));
        assert_eq!(
            map.key_for(ShortcutAction::ShowWord).to_upstream(),
            "Escape"
        );
    }

    #[test]
    fn reverse_lookup_returns_every_action_on_a_key() {
        let map = ShortcutMap::default();
        let key = map.key_for(ShortcutAction::KnowWord);
        let actions = map.actions_for(&key);
        assert!(actions.contains(&ShortcutAction::KnowWord));
        assert!(actions.contains(&ShortcutAction::ChooseA));
    }
}
