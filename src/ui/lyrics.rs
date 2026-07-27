//! Lyrics view rendering component.

use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::ui::{center_v, truncate};

pub fn render_lyrics(f: &mut Frame, app: &App, theme: Theme, area: Rect) {
    let inner = area.inner(Margin::new(2, 0));
    if inner.height == 0 {
        return;
    }
    let max = inner.width as usize;

    let mut lyrics_area = inner;
    if let Some(n) = app.now.as_ref() {
        let head = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate(&n.title, max),
                Style::default()
                    .fg(theme.text.into())
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            head[0],
        );
        let sub = if n.album.is_empty() {
            n.artist.clone()
        } else {
            format!("{} · {}", n.artist, n.album)
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(truncate(&sub, max), theme.muted())))
                .alignment(Alignment::Center),
            head[1],
        );
        lyrics_area = head[3];
    }

    if app.lyrics.is_empty() {
        let msg = if app.now.is_some() {
            "♪︎  no lyrics for this track"
        } else {
            "♪︎  nothing playing"
        };
        f.render_widget(
            Paragraph::new(msg)
                .style(theme.muted())
                .alignment(Alignment::Center),
            center_v(lyrics_area, 1),
        );
        return;
    }

    let h = lyrics_area.height as usize;
    let pos = app.position_ms();
    let cur = if app.lyrics_synced {
        app.lyrics.iter().rposition(|(t, _)| *t <= pos).unwrap_or(0)
    } else {
        0
    };
    let start = cur.saturating_sub(h / 2);

    let mut lines: Vec<Line> = Vec::with_capacity(h);
    for (i, (_, text)) in app.lyrics.iter().enumerate().skip(start).take(h) {
        let style = if app.lyrics_synced && i == cur {
            Style::default()
                .fg(theme.primary.into())
                .add_modifier(Modifier::BOLD)
        } else if app.lyrics_synced && i < cur {
            Style::default().fg(theme.border_subtle.into())
        } else {
            theme.muted()
        };
        let txt = if text.is_empty() {
            "♪︎".to_string()
        } else {
            truncate(text, max)
        };
        lines.push(Line::from(Span::styled(txt, style)));
    }
    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        lyrics_area,
    );
}
