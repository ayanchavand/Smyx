//! Now Playing pane rendering component.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::ui::overlays::{render_progress, render_volume};
use crate::ui::visualizer::render_visualizer;
use crate::ui::{center_v, fmt_ms, truncate};

pub fn render_nowplaying_view(f: &mut Frame, app: &mut App, theme: Theme, area: Rect) {
    if app.now.is_none() {
        let sel_item = app.cur_items().get(app.selected).cloned();
        if let Some(item) = sel_item.filter(|i| !i.is_header) {
            let chunks = Layout::vertical([
                Constraint::Min(6),
                Constraint::Length(7),
                Constraint::Length(2),
            ])
            .split(area);
            let top = chunks[0];
            let top = Rect {
                x: top.x,
                y: top.y + 3,
                width: top.width,
                height: top.height.saturating_sub(3),
            };

            let font = app.picker.font_size();
            let fw = font.width.max(1) as u32;
            let fh = font.height.max(1) as u32;

            let avail_h = top.height.saturating_sub(4);
            let mut art_h = avail_h.clamp(3, 14);
            let mut art_w = (art_h as u32 * fh / fw) as u16;
            if art_w > top.width {
                art_w = top.width;
                art_h = (art_w as u32 * fw / fh) as u16;
            }

            let group_h = art_h + 4;
            let art_y = top.y + top.height.saturating_sub(group_h) / 2;
            let art_x = top.x + top.width.saturating_sub(art_w) / 2;
            let art_rect = Rect {
                x: art_x,
                y: art_y,
                width: art_w,
                height: art_h,
            };

            if let Some((ref uri, cover)) = app.selected_cover.as_mut() {
                if uri == &item.uri {
                    f.render_widget(Clear, art_rect);
                    cover.render(f, art_rect);
                }
            }

            let text_rect = Rect {
                x: top.x,
                y: art_rect.y + art_h + 1,
                width: top.width,
                height: 3,
            };
            let lines = vec![
                Line::from(Span::styled(
                    truncate(&item.name, top.width as usize),
                    Style::default()
                        .fg(theme.text.into())
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    truncate(&item.subtitle, top.width as usize),
                    Style::default().fg(theme.primary.into()),
                )),
                Line::from(Span::styled("Press Enter to Play", theme.muted())),
            ];
            f.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center),
                text_rect,
            );
            return;
        }

        f.render_widget(
            Paragraph::new("Nothing playing.\nBrowse ← and press Enter.")
                .style(theme.muted())
                .alignment(Alignment::Center),
            center_v(area, 2),
        );
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Min(6),
        Constraint::Length(7),
        Constraint::Length(2),
    ])
    .split(area);
    let top = chunks[0];
    let top = Rect {
        x: top.x,
        y: top.y + 3,
        width: top.width,
        height: top.height.saturating_sub(3),
    };
    let viz_area = chunks[1];

    let font = app.picker.font_size();
    let fw = font.width.max(1) as u32;
    let fh = font.height.max(1) as u32;

    let avail_h = top.height.saturating_sub(4);
    let mut art_h = avail_h.clamp(3, 14);
    let mut art_w = (art_h as u32 * fh / fw) as u16;
    if art_w > top.width {
        art_w = top.width;
        art_h = (art_w as u32 * fw / fh) as u16;
    }

    let group_h = art_h + 4;
    let art_y = top.y + top.height.saturating_sub(group_h) / 2;
    let art_x = top.x + top.width.saturating_sub(art_w) / 2;
    let art_rect = Rect {
        x: art_x,
        y: art_y,
        width: art_w,
        height: art_h,
    };

    if let Some(cover) = app.now.as_mut().and_then(|n| n.cover.as_mut()) {
        f.render_widget(Clear, art_rect);
        cover.render(f, art_rect);
    }

    if let Some(n) = app.now.as_ref() {
        let text_rect = Rect {
            x: top.x,
            y: art_rect.y + art_h + 1,
            width: top.width,
            height: 3,
        };
        let lines = vec![
            Line::from(Span::styled(
                truncate(&n.title, top.width as usize),
                Style::default()
                    .fg(theme.text.into())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                truncate(&n.artist, top.width as usize),
                Style::default().fg(theme.primary.into()),
            )),
            Line::from(Span::styled(
                truncate(&n.album, top.width as usize),
                theme.muted(),
            )),
        ];
        f.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            text_rect,
        );
    }

    render_visualizer(f, app, theme, viz_area);
}

pub fn render_now_strip(f: &mut Frame, app: &mut App, theme: Theme, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    render_volume(f, app, theme, rows[0]);

    let pos = app.position_ms();
    let left_len = format!("{} ", fmt_ms(pos)).chars().count() as u16;
    let right_len = format!(
        " {}",
        fmt_ms(app.now.as_ref().map(|n| n.duration_ms).unwrap_or(0))
    )
    .chars()
    .count() as u16;
    let bar_w = rows[1].width.saturating_sub(left_len + right_len);
    app.bar_rect = Some(Rect {
        x: rows[1].x + left_len,
        y: rows[1].y,
        width: bar_w,
        height: 1,
    });
    render_progress(f, app, theme, rows[1]);
}
