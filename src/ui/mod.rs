//! UI rendering module for Myx.

pub mod library;
pub mod lyrics;
pub mod now_playing;
pub mod overlays;
pub mod queue;
pub mod visualizer;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::{Frame, Terminal};
use std::io::Stdout;

use crate::app::App;
use crate::components::gradient_line;
use crate::login_modal::render_login_modal;
use crate::models::RightView;
use crate::theme::Theme;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

pub fn render(f: &mut Frame, app: &mut App) {
    let theme = app.displayed;
    let area = f.area();
    f.render_widget(Block::default().style(theme.base()), area);
    let area = area.inner(Margin::new(2, 1));

    let rows = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Length(1), // spacer
        Constraint::Min(6),    // body (library | active view)
        Constraint::Length(1), // spacer
        Constraint::Length(2), // now-playing strip
        Constraint::Length(1), // footer
    ])
    .split(area);

    let mut header: Vec<Span> =
        gradient_line("\u{FF2D}\u{FF39}\u{FF38}", &[theme.primary, theme.accent])
            .into_iter()
            .map(|mut sp| {
                sp.style = sp.style.add_modifier(Modifier::BOLD);
                sp
            })
            .collect();
    if !app.status.is_empty() {
        header.push(Span::styled(format!("   {}", app.status), theme.muted()));
    }
    f.render_widget(Paragraph::new(Line::from(header)), rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(view_tabs(app, theme))).alignment(Alignment::Right),
        rows[0],
    );

    let mut total: usize = 3;
    for (i, v) in RightView::ALL.iter().enumerate() {
        if i > 0 {
            total += 3;
        }
        total += v.label().chars().count();
    }
    let mut tx_x = rows[0]
        .right()
        .saturating_sub(total as u16)
        .saturating_add(3);
    let mut tabs = Vec::with_capacity(RightView::ALL.len());
    for (i, v) in RightView::ALL.iter().enumerate() {
        if i > 0 {
            tx_x = tx_x.saturating_add(3);
        }
        let w = v.label().chars().count() as u16;
        tabs.push((
            *v,
            Rect {
                x: tx_x,
                y: rows[0].y,
                width: w,
                height: 1,
            },
        ));
        tx_x = tx_x.saturating_add(w);
    }
    app.tab_rects = tabs;

    let body = Layout::horizontal([Constraint::Percentage(30), Constraint::Min(24)])
        .spacing(3)
        .split(rows[2]);

    library::render_library(f, app, theme, body[0]);
    match app.view {
        RightView::NowPlaying => now_playing::render_nowplaying_view(f, app, theme, body[1]),
        RightView::Lyrics => lyrics::render_lyrics(f, app, theme, body[1]),
        RightView::Queue => queue::render_queue_view(f, app, theme, body[1]),
    }

    now_playing::render_now_strip(f, app, theme, rows[4]);
    overlays::render_footer(f, app, theme, rows[5]);

    if app.actions.is_some() {
        overlays::render_actions_overlay(f, app, theme, area);
    }

    if let Some(ref modal) = app.login_modal {
        render_login_modal(f, modal, &theme);
    }
}

fn view_tabs<'a>(app: &App, theme: Theme) -> Vec<Span<'a>> {
    let mut spans = vec![Span::styled("←→ ", theme.muted())];
    for (i, v) in RightView::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme.muted()));
        }
        let style = if *v == app.view {
            Style::default()
                .fg(theme.primary.into())
                .add_modifier(Modifier::BOLD)
        } else {
            theme.muted()
        };
        spans.push(Span::styled(v.label(), style));
    }
    spans
}

pub fn center_v(area: Rect, height: u16) -> Rect {
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x: area.x,
        y,
        width: area.width,
        height: height.min(area.height),
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

pub fn fmt_ms(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}
