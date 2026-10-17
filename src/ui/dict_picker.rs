//! 词库选择页。
//!
//! layout 与 render 共用同一组 Rect；鼠标命中检测只读 layout，
//! 因此绘制与点击区域不会随滚动漂移。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{Action, App};
use crate::domain::catalog::DictResource;
use crate::domain::types::Dict;
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text, wrap_text};

/// 内容宽度低于该值时隐藏右侧详情，只显示列表。
pub const DETAIL_MIN_WIDTH: u16 = 72;

#[derive(Clone, Debug)]
pub struct DictPickerLayout {
    pub list: Rect,
    pub rows: Vec<(usize, Rect)>,
    pub detail: Option<Rect>,
    pub activate: Option<Rect>,
}

pub fn layout(app: &App, content: Rect) -> DictPickerLayout {
    // 第一行是 section header，空一行后开始列表。
    let body = Rect::new(
        content.x,
        content.y.saturating_add(2),
        content.width,
        content.height.saturating_sub(2),
    );

    let (list, detail) = if content.width >= DETAIL_MIN_WIDTH {
        let list_width = body.width.saturating_mul(58) / 100;
        let detail = Rect::new(
            body.x.saturating_add(list_width).saturating_add(1),
            body.y,
            body.width.saturating_sub(list_width).saturating_sub(1),
            body.height,
        );
        (
            Rect::new(body.x, body.y, list_width, body.height),
            Some(detail),
        )
    } else {
        (body, None)
    };

    let total = app.boot.catalog.resources().len();
    let visible = list.height as usize;
    let offset = scroll_offset(app.dict_cursor, total, visible);
    let rows = (offset..total.min(offset.saturating_add(visible)))
        .map(|index| {
            (
                index,
                Rect::new(list.x, list.y + (index - offset) as u16, list.width, 1),
            )
        })
        .collect();

    let activate = detail.and_then(activate_rect);
    DictPickerLayout {
        list,
        rows,
        detail,
        activate,
    }
}

/// 让光标始终落在可视区域内的滚动偏移。
fn scroll_offset(cursor: usize, total: usize, visible: usize) -> usize {
    if total == 0 || visible == 0 {
        return 0;
    }
    cursor.min(total - 1).saturating_sub(visible - 1)
}

fn detail_inner(detail: Rect) -> Rect {
    Rect::new(
        detail.x.saturating_add(2),
        detail.y.saturating_add(1),
        detail.width.saturating_sub(3),
        detail.height.saturating_sub(2),
    )
}

fn activate_rect(detail: Rect) -> Option<Rect> {
    let inner = detail_inner(detail);
    if inner.width < 12 || inner.height < 2 {
        return None;
    }
    Some(Rect::new(
        inner.x,
        inner.bottom().saturating_sub(1),
        inner.width.min(16),
        1,
    ))
}

pub fn targets(app: &App, content: Rect) -> Vec<Target> {
    let layout = layout(app, content);
    let mut targets: Vec<Target> = layout
        .rows
        .iter()
        .map(|(index, rect)| {
            let action = if *index == app.dict_cursor {
                Action::ActivateDict(*index)
            } else {
                Action::SelectDict(*index)
            };
            Target::new(*rect, action)
        })
        .collect();
    if let Some(activate) = layout.activate {
        targets.push(Target::new(activate, Action::ActivateDict(app.dict_cursor)));
    }
    targets
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Dicts, |frame, content| {
        render_header(app, frame, content, theme);
        let layout = layout(app, content);
        render_list(app, frame, &layout, theme);
        if let Some(detail) = layout.detail {
            widgets::paint_rect(
                frame,
                Rect::new(layout.list.right(), layout.list.y, 1, layout.list.height),
                theme.border,
            );
            render_detail(app, frame, detail, layout.activate, theme);
        }
    });
}

fn render_header(app: &App, frame: &mut Frame, content: Rect, theme: &Theme) {
    let count = app.boot.catalog.resources().len();
    let header = widgets::section_header(
        "全部词库",
        &format!("共 {count} 个词库"),
        content.width as usize,
        theme,
    );
    frame.render_widget(
        Paragraph::new(header),
        Rect::new(content.x, content.y, content.width, 1),
    );
}

fn render_list(app: &App, frame: &mut Frame, layout: &DictPickerLayout, theme: &Theme) {
    let resources = app.boot.catalog.resources();
    if layout.rows.is_empty() {
        if layout.list.height > 0 {
            frame.render_widget(
                Paragraph::new(Span::styled("没有可用词库", Style::default().fg(theme.dim))),
                Rect::new(
                    layout.list.x.saturating_add(2),
                    layout.list.y,
                    layout.list.width.saturating_sub(2),
                    1,
                ),
            );
        }
        return;
    }
    for (index, rect) in &layout.rows {
        if let Some(resource) = resources.get(*index) {
            render_row(app, frame, *rect, *index, resource, theme);
        }
    }

    // 列表被裁切时在右侧提示还有更多词库（最后一列不参与行内容）。
    let total = resources.len();
    let indicator_x = layout.list.right().saturating_sub(1);
    if let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last())
        && layout.list.width > 0
    {
        if first.0 > 0 {
            frame.render_widget(
                Paragraph::new(Span::styled("↑", Style::default().fg(theme.accent_alt))),
                Rect::new(indicator_x, first.1.y, 1, 1),
            );
        }
        if last.0 + 1 < total {
            frame.render_widget(
                Paragraph::new(Span::styled("↓", Style::default().fg(theme.accent_alt))),
                Rect::new(indicator_x, last.1.y, 1, 1),
            );
        }
    }
}

fn render_row(
    app: &App,
    frame: &mut Frame,
    rect: Rect,
    index: usize,
    resource: &DictResource,
    theme: &Theme,
) {
    if rect.width < 10 || rect.height == 0 {
        return;
    }
    let selected = index == app.dict_cursor;
    if selected {
        widgets::paint_rect(frame, rect, theme.selection);
        widgets::paint_rect(
            frame,
            Rect::new(rect.x, rect.y, 1, rect.height),
            theme.accent,
        );
    }

    let current = is_current(app, resource);
    let owned = owned_dict(app, resource);
    let percent = owned.map(Dict::progress_percent).unwrap_or(0);
    let length = if resource.length > 0 {
        resource.length
    } else {
        owned.map(Dict::effective_length).unwrap_or(0)
    };

    // 右侧留一列给 ↑/↓ 滚动指示，避免压住进度百分比。
    let inner_width = rect.width.saturating_sub(4) as usize;
    let marker = if current {
        "●"
    } else if selected {
        "▶"
    } else {
        " "
    };
    let marker_style = Style::default().fg(if current {
        theme.accent
    } else if selected {
        theme.accent_alt
    } else {
        theme.dim
    });

    let length_text = format!("{length} 词");
    let percent_text = format!("{percent}%");
    let bar_width = if inner_width >= 44 { 6 } else { 4 };
    let tail_width = display_width(&length_text) + 1 + bar_width + 1 + display_width(&percent_text);
    let left_width = inner_width.saturating_sub(tail_width + 1);
    let mut name_space = left_width.saturating_sub(2);
    let show_id = name_space >= 18 && inner_width >= 40;
    let id_limit = if show_id { (name_space - 9).min(10) } else { 0 };
    if show_id {
        name_space = name_space.saturating_sub(id_limit + 1);
    }
    if name_space == 0 {
        return;
    }

    let name = fit_text(&resource.name, name_space);
    let id = if show_id {
        fit_text(&resource.id_string(), id_limit)
    } else {
        String::new()
    };

    let mut name_style = Style::default().fg(theme.fg);
    if selected {
        name_style = name_style.add_modifier(Modifier::BOLD);
    }

    let mut used = display_width(marker) + 1 + display_width(&name);
    let mut spans = vec![
        Span::styled(format!("{marker} "), marker_style),
        Span::styled(name, name_style),
    ];
    if show_id && !id.is_empty() {
        used += 1 + display_width(&id);
        spans.push(Span::raw(" "));
        spans.push(Span::styled(id, Style::default().fg(theme.dim)));
    }

    let gap = inner_width.saturating_sub(used + tail_width);
    spans.push(Span::raw(" ".repeat(gap)));
    let bar_color = if current {
        theme.accent
    } else {
        theme.accent_alt
    };
    spans.extend(widgets::thin_bar_spans(
        percent,
        bar_width,
        bar_color,
        theme.border,
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(length_text, Style::default().fg(theme.muted)));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        percent_text,
        Style::default().fg(if percent > 0 {
            theme.accent_alt
        } else {
            theme.dim
        }),
    ));

    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(
            rect.x.saturating_add(2),
            rect.y,
            inner_width as u16,
            rect.height.min(1),
        ),
    );
}

fn render_detail(
    app: &App,
    frame: &mut Frame,
    detail: Rect,
    activate: Option<Rect>,
    theme: &Theme,
) {
    if detail.width < 10 || detail.height < 2 {
        return;
    }
    widgets::paint_rect(frame, detail, theme.panel);
    widgets::paint_rect(
        frame,
        Rect::new(detail.x, detail.y, 1, detail.height),
        theme.accent,
    );
    let inner = detail_inner(detail);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(resource) = app.boot.catalog.resources().get(app.dict_cursor) else {
        frame.render_widget(
            Paragraph::new(Span::styled("未选择词库", Style::default().fg(theme.dim))),
            inner,
        );
        return;
    };
    let text_height = if activate.is_some() {
        inner.height.saturating_sub(2)
    } else {
        inner.height
    };
    let lines = detail_lines(app, resource, inner.width as usize, theme);
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(inner.x, inner.y, inner.width, text_height),
    );
    if let Some(activate) = activate {
        widgets::draw_button(
            frame,
            activate,
            "Enter 设为当前",
            theme.accent,
            theme.bg,
            true,
        );
    }
}

fn detail_lines(
    app: &App,
    resource: &DictResource,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let owned = owned_dict(app, resource);
    let current = is_current(app, resource);
    let percent = owned.map(Dict::progress_percent).unwrap_or(0);
    let length = if resource.length > 0 {
        resource.length
    } else {
        owned.map(Dict::effective_length).unwrap_or(0)
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut name_spans: Vec<Span<'static>> = Vec::new();
    if current {
        name_spans.push(Span::styled("● ", Style::default().fg(theme.accent)));
    }
    name_spans.push(Span::styled(
        fit_text(&resource.name, width),
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::from(name_spans));

    let mut meta: Vec<String> = Vec::new();
    let id = resource.id_string();
    if !id.is_empty() {
        meta.push(id);
    }
    if !resource.category.is_empty() {
        meta.push(resource.category.clone());
    }
    if !meta.is_empty() {
        lines.push(Line::from(Span::styled(
            fit_text(&meta.join(" · "), width),
            Style::default().fg(theme.muted),
        )));
    }

    if !resource.description.is_empty() {
        lines.push(Line::from(""));
        for line in wrap_text(&resource.description, width) {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(theme.muted),
            )));
        }
    }

    if !resource.tags.is_empty() {
        lines.push(Line::from(""));
        let tags = format!("标签 {}", resource.tags.join(" · "));
        for line in wrap_text(&tags, width) {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(theme.dim),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        fit_text(&format!("词数 {length} 词"), width),
        Style::default().fg(theme.fg),
    )));

    let percent_text = format!("{percent}%");
    let bar_width = width
        .saturating_sub(display_width(&percent_text) + 1)
        .max(6);
    let mut progress = widgets::thin_bar_spans(
        percent,
        bar_width,
        if current {
            theme.accent
        } else {
            theme.accent_alt
        },
        theme.border,
    );
    progress.push(Span::raw(" "));
    progress.push(Span::styled(percent_text, Style::default().fg(theme.muted)));
    lines.push(Line::from(progress));

    let daily = match owned {
        Some(dict) if current => format!("当前每日词数 {} 词", dict.per_day()),
        Some(dict) => format!("每日词数 {} 词", dict.per_day()),
        None => "设为当前后可调整每日词数".to_string(),
    };
    lines.push(Line::from(Span::styled(
        fit_text(&daily, width),
        Style::default().fg(if current {
            theme.accent_alt
        } else {
            theme.muted
        }),
    )));

    lines
}

fn is_current(app: &App, resource: &DictResource) -> bool {
    let id = resource.id_string();
    app.boot
        .base
        .study_dict()
        .map(|dict| dict.matches_id(&id) || dict.matches_id(&resource.en_name))
        .unwrap_or(false)
}

fn owned_dict<'a>(app: &'a App, resource: &DictResource) -> Option<&'a Dict> {
    let id = resource.id_string();
    app.boot
        .base
        .word
        .book_list
        .iter()
        .find(|dict| dict.matches_id(&id) || dict.matches_id(&resource.en_name))
}
