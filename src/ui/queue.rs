//! Queue view rendering component.

use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::ui::truncate;

pub fn render_queue_view(f: &mut Frame, app: &App, theme: Theme, area: Rect) {
    let inner = area.inner(Margin::new(2, 1));
    if inner.height == 0 {
        return;
    }
    let max = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    if !app.source_name.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("PLAYING FROM  ", theme.muted()),
            Span::styled(
                truncate(&app.source_name, max.saturating_sub(14)),
                Style::default()
                    .fg(theme.primary.into())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::raw(""));
    }

    if let Some(n) = app.now.as_ref() {
        lines.push(Line::from(Span::styled("NOW PLAYING", theme.heading())));
        lines.push(Line::from(vec![
            Span::styled("   ", theme.muted()),
            Span::styled(
                truncate(&n.title, max.saturating_sub(3)),
                Style::default()
                    .fg(theme.text.into())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", n.artist), theme.muted()),
        ]));
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(Span::styled("UP NEXT", theme.heading())));
    lines.push(Line::raw(""));

    let used = lines.len();
    if app.queue.is_empty() {
        lines.push(Line::from(Span::styled("queue is empty", theme.muted())));
    } else {
        for (i, q) in app
            .queue
            .iter()
            .take(inner.height.saturating_sub(used as u16) as usize)
            .enumerate()
        {
            lines.push(Line::from(vec![
                Span::styled(format!("{:>2}  ", i + 1), theme.muted()),
                Span::styled(
                    truncate(q, max.saturating_sub(4)),
                    Style::default().fg(theme.text.into()),
                ),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}
