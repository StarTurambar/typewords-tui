//! 设置页：分区滚动的设置列表。
//!
//! layout 与 render 共用同一组条目行；targets 只读 layout。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{Action, App, ConfirmAction, EditTarget, SETTING_ROWS, SettingKind};
use crate::domain::constants::practice_mode_name;
use crate::domain::i18n::tr;
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

/// 分区起点：设置项 key -> 分区标题。顺序即展示顺序。
const SECTIONS: &[(&str, &str)] = &[
    ("word_practice_mode", "通用"),
    ("keyboard_sound", "音频"),
    ("daily_words", "复习"),
    ("auto_next_word", "键入"),
    ("show_word_question", "显示"),
    ("fsrs_easy_limit", "FSRS"),
    ("reset_practice_progress", "其他"),
];

#[derive(Clone, Debug)]
pub struct SettingsLayout {
    pub offset: usize,
    pub rows: Vec<(Option<usize>, Rect)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Section(&'static str),
    Item(usize),
}

fn section_starts() -> Vec<(&'static str, usize)> {
    let mut starts = Vec::new();
    for (key, title) in SECTIONS {
        if let Some(index) = SETTING_ROWS.iter().position(|(row_key, _)| row_key == key) {
            starts.push((*title, index));
        }
    }
    starts
}

fn entries() -> Vec<Entry> {
    let starts = section_starts();
    let mut entries = Vec::new();
    let mut next_section = 0usize;
    for (index, _) in SETTING_ROWS.iter().enumerate() {
        if let Some((title, start)) = starts.get(next_section).copied()
            && index == start
        {
            entries.push(Entry::Section(title));
            next_section += 1;
        }
        entries.push(Entry::Item(index));
    }
    entries
}

pub fn layout(app: &App, content: Rect) -> SettingsLayout {
    let entries = entries();
    let visible = content.height as usize;
    let cursor_entry = entries
        .iter()
        .position(|entry| matches!(entry, Entry::Item(index) if *index == app.settings_cursor))
        .unwrap_or(0);

    let mut offset = cursor_entry.saturating_sub(visible.saturating_sub(1));
    if offset > 0 {
        // 尽量让分区标题跟着当前条目一起滚入视口。
        let start = entries[..=offset]
            .iter()
            .rposition(|entry| matches!(entry, Entry::Section(_)))
            .unwrap_or(0);
        if cursor_entry - start < visible {
            offset = start;
        }
    }

    let rows = entries
        .iter()
        .skip(offset)
        .take(visible)
        .enumerate()
        .map(|(position, entry)| {
            let item = match entry {
                Entry::Item(index) => Some(*index),
                Entry::Section(_) => None,
            };
            (
                item,
                Rect::new(
                    content.x,
                    content.y + position as u16,
                    // 最右 1 列留给滚动条：选中高亮与点击都不覆盖它。
                    content.width.saturating_sub(1),
                    1,
                ),
            )
        })
        .collect();

    SettingsLayout { offset, rows }
}

pub fn targets(app: &App, content: Rect) -> Vec<Target> {
    let layout = layout(app, content);
    layout
        .rows
        .iter()
        .filter_map(|(entry, rect)| {
            let index = (*entry)?;
            let action = if index == app.settings_cursor {
                Action::ActivateSetting(index)
            } else {
                Action::SelectSetting(index)
            };
            // 行矩形已在布局里让出最右一列（滚动条）。
            Some(Target::new(*rect, action))
        })
        .collect()
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Settings, |frame, content| {
        let layout = layout(app, content);
        let entries = entries();
        for (position, (_, rect)) in layout.rows.iter().enumerate() {
            match entries.get(layout.offset + position) {
                Some(Entry::Section(title)) => {
                    render_section(frame, *rect, title, theme);
                }
                Some(Entry::Item(index)) => {
                    render_row(app, frame, *rect, *index, theme);
                }
                None => {}
            }
        }

        widgets::scrollbar(
            frame,
            Rect::new(
                content.right().saturating_sub(1),
                content.y,
                1,
                content.height,
            ),
            widgets::ScrollbarState::new(layout.offset, entries.len(), content.height as usize),
            theme,
        );
    });
}

fn render_section(frame: &mut Frame, rect: Rect, title: &str, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let prefix = fit_text(&format!("▎ {title} "), rect.width as usize);
    let prefix_width = display_width(&prefix);
    let rule_width = (rect.width as usize).saturating_sub(prefix_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(rule_width), Style::default().fg(theme.border)),
        ])),
        rect,
    );
}

fn render_row(app: &App, frame: &mut Frame, rect: Rect, index: usize, theme: &Theme) {
    let Some((key, kind)) = SETTING_ROWS.get(index).copied() else {
        return;
    };
    if rect.width < 8 || rect.height == 0 {
        return;
    }
    let selected = index == app.settings_cursor;
    if selected {
        widgets::paint_rect(frame, rect, theme.selection);
        widgets::paint_rect(
            frame,
            Rect::new(rect.x, rect.y, 1, rect.height),
            theme.accent,
        );
    }

    let inner_x = rect.x.saturating_add(2);
    // 行矩形已扣掉最右一列滚动条，再留 4 列空白，避免数值贴着滚动条。
    let inner_width = rect.width.saturating_sub(4) as usize;
    if inner_width == 0 {
        return;
    }

    let label_text = tr(key);
    let (value_text, value_color) = setting_value(app, key, kind, theme);
    let value = fit_text(&value_text, inner_width.saturating_sub(12).max(1));
    let value_width = display_width(&value);
    let label = fit_text(&label_text, inner_width.saturating_sub(value_width + 1));
    let label_width = display_width(&label);
    let gap = inner_width.saturating_sub(label_width + value_width);

    let label_style = if selected {
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(label, label_style),
            Span::raw(" ".repeat(gap)),
            Span::styled(value, Style::default().fg(value_color)),
        ])),
        Rect::new(inner_x, rect.y, inner_width as u16, 1),
    );
}

fn setting_value(app: &App, key: &str, kind: SettingKind, theme: &Theme) -> (String, Color) {
    match kind {
        SettingKind::Mode => (
            practice_mode_name(app.boot.settings.word_practice_mode).to_string(),
            theme.accent_alt,
        ),
        SettingKind::Theme => (app.theme_label(), theme.accent_alt),
        SettingKind::SoundType => (
            if app.boot.settings.sound_type == "uk" {
                "英式 (uk)".to_string()
            } else {
                "美式 (us)".to_string()
            },
            theme.accent_alt,
        ),
        SettingKind::Bool => {
            let enabled = setting_bool(app, key);
            (
                if enabled { "开启" } else { "关闭" }.to_string(),
                if enabled { theme.good } else { theme.muted },
            )
        }
        SettingKind::Number(target) => (
            number_text(target, app.setting_number_value(target)),
            theme.fg,
        ),
        SettingKind::Action(action) => action_hint(action, theme),
        SettingKind::Shortcuts => ("按 Enter 打开".to_string(), theme.dim),
        SettingKind::AdvancedFsrs => ("按 Enter 打开".to_string(), theme.dim),
        SettingKind::AudioSource => (
            if app.boot.settings.tts_source == "youdao" {
                "有道发音（需网络）".to_string()
            } else {
                "系统 TTS".to_string()
            },
            theme.accent_alt,
        ),
    }
}

/// 动作行的右侧提示：文案与配色都要交代这个动作到底做什么。
///
/// 早期实现把所有动作统一写成「按 Enter 重置」，「清除所有数据」（不可撤销）
/// 于是看起来和「清空当前练习进度」（只删断点缓存）一样轻。现在各自用自己的
/// 动词，危险动作再用 `bad` 色标出来。
///
/// 这里刻意穷举 `ConfirmAction` 且不写 `_` 兜底：将来新增动作时编译器会
/// 逼着补一条文案，而不是悄悄落回同一句话。
fn action_hint(action: ConfirmAction, theme: &Theme) -> (String, Color) {
    match action {
        ConfirmAction::Quit => ("按 Enter 退出".to_string(), theme.dim),
        ConfirmAction::ResetPractice => ("按 Enter 重置".to_string(), theme.dim),
        ConfirmAction::ClearAllData => ("按 Enter 清除 · 不可撤销".to_string(), theme.bad),
        // 「恢复默认 FSRS 参数」只出现在高级 FSRS 页，不占设置页的行。
        ConfirmAction::ResetFsrs => ("按 Enter 恢复".to_string(), theme.warn),
    }
}

fn setting_bool(app: &App, key: &str) -> bool {
    let settings = &app.boot.settings;
    match key {
        "auto_random_review" => settings.auto_add_random_review_when_no_due,
        "ignore_simple_word" => settings.ignore_simple_word,
        "auto_next_word" => settings.auto_next_word,
        "input_wrong_clear" => settings.input_wrong_clear,
        "ignore_case" => settings.ignore_case,
        "show_word_question" => settings.show_word_question,
        "show_etymology" => settings.show_etymology_and_rel_words,
        "show_near_word" => settings.show_near_word,
        "show_usage_tips" => settings.show_usage_tips,
        "big_word" => settings.tui_big_word,
        "keyboard_sound" => settings.keyboard_sound,
        "effect_sound" => settings.effect_sound,
        "word_sound" => settings.word_sound,
        "article_sound" => settings.article_sound,
        _ => false,
    }
}

fn number_text(target: EditTarget, value: i64) -> String {
    match target {
        EditTarget::Ratio => value.to_string(),
        EditTarget::Repeat => format!("{value} 次"),
        EditTarget::RepeatCustom => format!("{value} 次"),
        EditTarget::WaitMs | EditTarget::SpaceCooldown => format!("{value} ms"),
        EditTarget::EasyLimit | EditTarget::GoodLimit | EditTarget::HardLimit => value.to_string(),
        EditTarget::Retention => format!("{value}%"),
        EditTarget::PerDay => format!("{value} 词"),
        EditTarget::Note => String::new(),
        EditTarget::WordSoundSpeed => format!("{value}%"),
        EditTarget::DictName => String::new(),
        EditTarget::MaximumInterval => format!("{value} 天"),
        // 权重是浮点，只在「高级 FSRS」页显示（`ui::adv_fsrs`），这里用不到。
        EditTarget::Weight(_) => value.to_string(),
    }
}
