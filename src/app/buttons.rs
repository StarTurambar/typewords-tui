//! 键入练习页（背单词 / 文章跟打）底部的动作按钮。
//!
//! 两个键入页原本只有快捷键：键位只出现在底部提示里，鼠标完全够不到。这里把
//! 「常用、可重复触发」的动作做成按钮——`←` / `→` / `↑` / `↓` 选中、`Enter`
//! 执行、鼠标点击直接执行，与结算 / 确认弹层是同一套操作语言。
//!
//! 按钮条**不抢键入**：只有方向键与「已选中时的 `Enter`」属于按钮条，其余按键
//! 照旧进键入引擎与键位表，并顺手取消选中——用户继续打字或按快捷键时，`Enter`
//! 不该还去执行上一个按钮。

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{App, FocusDir, Screen};
use crate::domain::shortcut::ShortcutAction;

/// 底部动作按钮的身份。
///
/// 大多数按钮就是键位表里的一个动作（见 [`TypingButton::shortcut`]），执行时走
/// 与键盘**同一条**处理路径；`Pause` / `Help` / `Return` 是 TUI 专有键位，
/// 没有键位表条目，键名由 [`TypingButton::fixed_key`] 给出。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypingButton {
    /// 显示单词（练习页）
    ShowWord,
    /// 跳过当前词
    Ignore,
    /// 标记 / 取消「已掌握」
    Mastered,
    /// 收藏
    Collect,
    /// 切换翻译（练习页）/ 当前句译文（跟打页）
    Translate,
    /// 切换默写遮罩（练习页）
    Dictation,
    /// 暂停 / 恢复计时
    Pause,
    /// 重学本章（练习页）
    Repeat,
    /// 换到下一组（练习页）
    NextGroup,
    /// 朗读当前词 / 当前句
    Speak,
    /// 切换主题
    Theme,
    /// 整屏帮助
    Help,
    /// 保存并离开当前练习页
    Return,
}

impl TypingButton {
    /// 按钮上的动作名（键名由 `ui::typing_buttons` 拼在前面）。
    pub fn label(self) -> &'static str {
        match self {
            TypingButton::ShowWord => "显示",
            TypingButton::Ignore => "跳过",
            TypingButton::Mastered => "掌握",
            TypingButton::Collect => "收藏",
            TypingButton::Translate => "翻译",
            TypingButton::Dictation => "默写",
            TypingButton::Pause => "暂停",
            TypingButton::Repeat => "重学",
            TypingButton::NextGroup => "换组",
            TypingButton::Speak => "朗读",
            TypingButton::Theme => "主题",
            TypingButton::Help => "帮助",
            TypingButton::Return => "返回",
        }
    }

    /// 键位表里对应的动作；`None` 表示是 TUI 专有键位。
    pub fn shortcut(self) -> Option<ShortcutAction> {
        match self {
            TypingButton::ShowWord => Some(ShortcutAction::ShowWord),
            TypingButton::Ignore => Some(ShortcutAction::Ignore),
            TypingButton::Mastered => Some(ShortcutAction::ToggleSimple),
            TypingButton::Collect => Some(ShortcutAction::ToggleCollect),
            TypingButton::Translate => Some(ShortcutAction::ToggleShowTranslate),
            TypingButton::Dictation => Some(ShortcutAction::ToggleDictation),
            TypingButton::Repeat => Some(ShortcutAction::RepeatChapter),
            TypingButton::NextGroup => Some(ShortcutAction::NextChapter),
            TypingButton::Speak => Some(ShortcutAction::PlayWordPronunciation),
            TypingButton::Theme => Some(ShortcutAction::ToggleTheme),
            TypingButton::Pause | TypingButton::Help | TypingButton::Return => None,
        }
    }

    /// TUI 专有键位的展示写法（两个练习页共用同一个键）。
    pub fn fixed_key(self) -> Option<&'static str> {
        match self {
            TypingButton::Pause => Some(crate::ui::TUI_PAUSE_KEY),
            TypingButton::Help => Some(crate::ui::TUI_HELP_KEY),
            TypingButton::Return => Some(crate::ui::TUI_EXIT_KEY),
            _ => None,
        }
    }
}

/// 背单词页的按钮，行优先、两行（上排是「看词 / 换词」，下排是「显示与收尾」）。
pub const PRACTICE_BUTTONS: [TypingButton; 10] = [
    TypingButton::ShowWord,
    TypingButton::Ignore,
    TypingButton::Mastered,
    TypingButton::Collect,
    TypingButton::Translate,
    TypingButton::Dictation,
    TypingButton::Pause,
    TypingButton::Repeat,
    TypingButton::NextGroup,
    TypingButton::Return,
];

/// 文章跟打页的按钮，行优先、两行。
pub const ARTICLE_BUTTONS: [TypingButton; 6] = [
    TypingButton::Speak,
    TypingButton::Translate,
    TypingButton::Theme,
    TypingButton::Pause,
    TypingButton::Help,
    TypingButton::Return,
];

/// 按钮条的列数：固定两行，列数按按钮数量均分（10 个 → 5 列，6 个 → 3 列）。
///
/// 绘制（`ui::typing_buttons`）与键盘上下移动共用它，布局与按键不会各算一套。
pub fn columns(count: usize) -> usize {
    count.div_ceil(2).max(1)
}

/// 不带修饰键的方向键 → 按钮条的移动方向；其它按键返回 `None`。
///
/// 只有裸方向键属于按钮条：`Ctrl+←` / `Alt+→` / `Shift+→` 是键位表里的
/// 上一个词 / 下一章 / 下一阶段。
pub(crate) fn plain_arrow_direction(key: &KeyEvent) -> Option<FocusDir> {
    if !key.modifiers.is_empty() {
        return None;
    }
    match key.code {
        KeyCode::Left => Some(FocusDir::Left),
        KeyCode::Right => Some(FocusDir::Right),
        KeyCode::Up => Some(FocusDir::Up),
        KeyCode::Down => Some(FocusDir::Down),
        _ => None,
    }
}

impl App {
    /// 当前页面显示的动作按钮；不显示按钮条的页面返回空切片。
    ///
    /// 工具栏关掉时练习页不画按钮条，方向键也就不该选中一个看不见的按钮。
    pub fn typing_buttons(&self) -> &'static [TypingButton] {
        match self.screen {
            Screen::Practice if self.show_toolbar => &PRACTICE_BUTTONS,
            Screen::ArticlePractice => &ARTICLE_BUTTONS,
            _ => &[],
        }
    }

    /// 按钮条的键盘处理：已选中时的 `Enter` 执行按钮；其它按键取消选中。
    ///
    /// 返回 `true` 表示按键已被按钮条消费。返回 `false` 时按键继续走原来的
    /// 分流（键位表 → TUI 键位 → 键入引擎）。
    ///
    /// **方向键不在这里处理**：它们由调用方在「键位表 / 键入都没人要」之后
    /// 交给 [`App::move_typing_button`]。这样用户把「上一个词」绑到 `←` 时，
    /// 键位表优先，按钮条不会把显式绑定抢走；没有绑定时方向键才是按钮条的。
    /// 只有不带修饰键的方向键属于按钮条——`Ctrl+←` / `Alt+→` / `Shift+→`
    /// 是键位表里的上一个词 / 下一章 / 下一阶段。
    pub(crate) fn handle_typing_button_key(&mut self, key: KeyEvent) -> bool {
        if self.typing_buttons().is_empty() {
            self.typing_button = None;
            return false;
        }
        if key.modifiers.is_empty() && key.code == KeyCode::Enter {
            if let Some(index) = self.typing_button {
                self.activate_typing_button(index);
                return true;
            }
            // 没有选中时 `Enter` 保持原来的语义（练习页 = 收藏）。
            return false;
        }
        if plain_arrow_direction(&key).is_none() {
            // 继续打字 / 按快捷键：取消选中，`Enter` 回到旧语义。
            self.typing_button = None;
        }
        false
    }

    /// 移动按钮选中：到边停、不回绕；没有选中时任何方向都落在第一个按钮。
    pub(crate) fn move_typing_button(&mut self, direction: FocusDir) {
        let count = self.typing_buttons().len();
        if count == 0 {
            return;
        }
        let columns = columns(count).max(1);
        let Some(current) = self.typing_button.filter(|index| *index < count) else {
            self.typing_button = Some(0);
            return;
        };
        let column = current % columns;
        let next = match direction {
            FocusDir::Left if column > 0 => Some(current - 1),
            FocusDir::Right if column + 1 < columns && current + 1 < count => Some(current + 1),
            FocusDir::Up if current >= columns => Some(current - columns),
            FocusDir::Down if current + columns < count => Some(current + columns),
            _ => None,
        };
        if let Some(next) = next {
            self.typing_button = Some(next);
        }
    }

    /// 执行第 `index` 个按钮：键盘 `Enter` 与鼠标点击共用这一个入口。
    ///
    /// 选中先落到被执行的按钮上（鼠标点哪个就高亮哪个），再走该页面的处理
    /// 分支；分支里换页（返回 / 帮助）会由 `set_screen` 清掉选中。
    pub(crate) fn activate_typing_button(&mut self, index: usize) {
        let Some(button) = self.typing_buttons().get(index).copied() else {
            return;
        };
        self.typing_button = Some(index);
        match self.screen {
            Screen::Practice => self.activate_practice_button(button),
            Screen::ArticlePractice => self.activate_article_button(button),
            _ => {}
        }
    }

    /// 背单词页的按钮：能对上键位表的动作走与键盘同一条路径。
    fn activate_practice_button(&mut self, button: TypingButton) {
        if let Some(action) = button.shortcut()
            && self.handle_practice_shortcut(action)
        {
            return;
        }
        match button {
            TypingButton::Pause => self.toggle_practice_pause(),
            TypingButton::Help => self.open_typing_help(),
            TypingButton::Return => self.save_and_leave_practice(),
            _ => {}
        }
    }

    /// 文章跟打页的按钮：同样先走键位表里两页共用的动作。
    fn activate_article_button(&mut self, button: TypingButton) {
        if let Some(action) = button.shortcut()
            && self.handle_article_shortcut(action)
        {
            return;
        }
        match button {
            TypingButton::Pause => self.toggle_article_pause(),
            TypingButton::Help => self.open_typing_help(),
            TypingButton::Return => self.back(),
            _ => {}
        }
    }
}
