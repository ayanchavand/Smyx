//! Volume bar, footer, progress bar, and action menu overlays.

use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{enter_label, App};
use crate::components::gradient_progress;
use crate::theme::Theme;
use crate::ui::{fmt_ms, truncate};

pub fn render_volume(f: &mut Frame, app: &mut App, theme: Theme, area: Rect) {
    const VLEV: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let filled = (app.volume as usize * VLEV.len() + 50) / 100;
    let mut vspans: Vec<Span> = Vec::with_capacity(VLEV.len() + 1);
    for (i, ch) in VLEV.iter().enumerate() {
        let color = if i < filled {
            theme.primary
        } else {
            theme.border_dimmest
        };
        vspans.push(Span::styled(
            ch.to_string(),
            Style::default().fg(color.into()),
        ));
    }
    vspans.push(Span::styled(format!(" {:>3}%", app.volume), theme.muted()));
    f.render_widget(
        Paragraph::new(Line::from(vspans)).alignment(Alignment::Right),
        area,
    );
    app.vol_rect = Some(Rect {
        x: area.right().saturating_sub(13),
        y: area.y,
        width: VLEV.len() as u16,
        height: 1,
    });
}

pub fn render_progress(f: &mut Frame, app: &App, theme: Theme, area: Rect) {
    let (pos, dur) = match &app.now {
        Some(n) => (app.position_ms(), n.duration_ms.max(1)),
        None => (0, 1),
    };
    let left = format!("{} ", fmt_ms(pos));
    let right = format!(" {}", fmt_ms(dur));
    let reserve = left.chars().count() + right.chars().count();
    let bar_w = (area.width as usize).saturating_sub(reserve);
    let filled = ((pos as f32 / dur as f32) * bar_w as f32) as usize;

    let mut spans = vec![Span::styled(left, theme.muted())];
    spans.extend(gradient_progress(
        bar_w,
        filled,
        &[theme.primary, theme.accent],
        theme.border_dimmest,
    ));
    spans.push(Span::styled(right, theme.muted()));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn render_footer(f: &mut Frame, app: &App, theme: Theme, area: Rect) {
    let on = |b: bool| if b { theme.success } else { theme.text_muted };
    let key = |k: &'static str| Span::styled(k, Style::default().fg(theme.primary.into()));
    let lbl = |t: &'static str| Span::styled(t, theme.muted());
    let enter_lbl = enter_label(app.cur_items().get(app.selected));
    let line = Line::from(vec![
        key("⇥"),
        lbl(" section   "),
        key("←→"),
        lbl(" view   "),
        key("/"),
        lbl(" search   "),
        key("⏎"),
        Span::styled(format!(" {enter_lbl}   "), theme.muted()),
        key("P"),
        lbl(" play   "),
        key("S"),
        lbl(" shuffle   "),
        key("␣"),
        Span::styled(if app.now.as_ref().is_some_and(|n| n.is_playing) { " pause   " } else { " play    " }, theme.muted()),
        key("n/b"),
        lbl(" skip   "),
        key("⇧←→"),
        lbl(" seek   "),
        key("o"),
        lbl(" sort   "),
        key("+/-"),
        lbl(" vol   "),
        Span::styled("s", Style::default().fg(on(app.shuffle).into())),
        lbl(" shuffle   "),
        key("a"),
        lbl(" actions   "),
        key("q"),
        lbl(" quit"),
    ]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

pub fn render_actions_overlay(f: &mut Frame, app: &App, theme: Theme, area: Rect) {
    let Some(menu) = &app.actions else { return };
    let w = (area.width * 5 / 10).clamp(28, 52);
    let h = (menu.items.len() as u16 + 4).clamp(6, area.height.saturating_sub(2));
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    f.render_widget(Clear, rect);
    f.render_widget(Block::default().style(theme.element()), rect);
    let inner = rect.inner(Margin::new(2, 1));
    let max = inner.width as usize;
    let mut lines = vec![
        Line::from(Span::styled(truncate(&menu.title, max), theme.heading())),
        Line::raw(""),
    ];
    for (i, it) in menu
        .items
        .iter()
        .take(inner.height.saturating_sub(2) as usize)
        .enumerate()
    {
        if i == menu.selected {
            lines.push(Line::from(vec![
                Span::styled("› ", Style::default().fg(theme.primary.into())),
                Span::styled(
                    truncate(&it.label, max.saturating_sub(2)),
                    Style::default()
                        .fg(theme.text.into())
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {}", truncate(&it.label, max.saturating_sub(2))),
                theme.muted(),
            )));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}
