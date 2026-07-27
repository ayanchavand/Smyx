//! Interactive TUI Login Modal component for Navidrome / OpenSubsonic.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::config::NavidromeConfig;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    ServerUrl = 0,
    Username = 1,
    Password = 2,
    Submit = 3,
}

impl LoginField {
    pub fn next(self) -> Self {
        match self {
            Self::ServerUrl => Self::Username,
            Self::Username => Self::Password,
            Self::Password => Self::Submit,
            Self::Submit => Self::ServerUrl,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::ServerUrl => Self::Submit,
            Self::Username => Self::ServerUrl,
            Self::Password => Self::Username,
            Self::Submit => Self::Password,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoginModalState {
    pub server_url: String,
    pub username: String,
    pub password: String,
    pub active_field: LoginField,
    pub error_message: Option<String>,
    pub is_connecting: bool,
}

pub enum LoginModalAction {
    None,
    Submit(NavidromeConfig),
}

impl Default for LoginModalState {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:4533".to_string(),
            username: String::new(),
            password: String::new(),
            active_field: LoginField::ServerUrl,
            error_message: None,
            is_connecting: false,
        }
    }
}

impl LoginModalState {
    pub fn from_config(config: &NavidromeConfig) -> Self {
        Self {
            server_url: config.server_url.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
            active_field: LoginField::Submit,
            error_message: None,
            is_connecting: false,
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> LoginModalAction {
        if self.is_connecting {
            return LoginModalAction::None;
        }

        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.active_field = self.active_field.next();
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.active_field = self.active_field.prev();
            }
            KeyCode::Enter => {
                return self.submit();
            }
            KeyCode::Char(c) => {
                match self.active_field {
                    LoginField::ServerUrl => self.server_url.push(c),
                    LoginField::Username => self.username.push(c),
                    LoginField::Password => self.password.push(c),
                    LoginField::Submit => {
                        return self.submit();
                    }
                }
            }
            KeyCode::Backspace => {
                match self.active_field {
                    LoginField::ServerUrl => {
                        self.server_url.pop();
                    }
                    LoginField::Username => {
                        self.username.pop();
                    }
                    LoginField::Password => {
                        self.password.pop();
                    }
                    LoginField::Submit => {}
                }
            }
            KeyCode::Esc => {
                // Clear active error message on Esc
                self.error_message = None;
            }
            _ => {}
        }

        LoginModalAction::None
    }

    fn submit(&mut self) -> LoginModalAction {
        if self.server_url.trim().is_empty() {
            self.error_message = Some("Server IP / URL cannot be empty".to_string());
            self.active_field = LoginField::ServerUrl;
            return LoginModalAction::None;
        }
        if self.username.trim().is_empty() {
            self.error_message = Some("Username cannot be empty".to_string());
            self.active_field = LoginField::Username;
            return LoginModalAction::None;
        }

        let config = NavidromeConfig::new(
            self.server_url.clone(),
            self.username.clone(),
            self.password.clone(),
        );

        LoginModalAction::Submit(config)
    }
}

/// Render the Login Modal overlay onto the frame.
pub fn render_login_modal(frame: &mut Frame, state: &LoginModalState, theme: &Theme) {
    let area = frame.area();

    // Center popup box constraints (width: 60 columns, height: 18 lines)
    let modal_width = 60.min(area.width.saturating_sub(4));
    let modal_height = 18.min(area.height.saturating_sub(2));

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(modal_height)) / 2),
            Constraint::Length(modal_height),
            Constraint::Min(0),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(modal_width)) / 2),
            Constraint::Length(modal_width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);

    let popup_area = horizontal[1];

    // Clear background beneath modal
    frame.render_widget(Clear, popup_area);

    let active_style = Style::default()
        .fg(theme.accent.into())
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(theme.text_muted.into());

    let block = Block::default()
        .title(" Navidrome Server Login ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent.into()))
        .style(Style::default().bg(theme.background.into()));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Server URL
            Constraint::Length(3), // Username
            Constraint::Length(3), // Password
            Constraint::Length(3), // Submit Button / Status
            Constraint::Min(1),    // Error message
        ])
        .split(inner);

    // 1. Server URL field
    let server_style = if state.active_field == LoginField::ServerUrl {
        active_style
    } else {
        inactive_style
    };
    let server_block = Block::default()
        .title(" Server IP / URL (e.g. http://localhost:4533) ")
        .borders(Borders::ALL)
        .border_style(server_style);
    let server_text = Paragraph::new(state.server_url.as_str())
        .style(Style::default().fg(theme.text.into()))
        .block(server_block);
    frame.render_widget(server_text, chunks[0]);

    // 2. Username field
    let user_style = if state.active_field == LoginField::Username {
        active_style
    } else {
        inactive_style
    };
    let user_block = Block::default()
        .title(" Username ")
        .borders(Borders::ALL)
        .border_style(user_style);
    let user_text = Paragraph::new(state.username.as_str())
        .style(Style::default().fg(theme.text.into()))
        .block(user_block);
    frame.render_widget(user_text, chunks[1]);

    // 3. Password field
    let pass_style = if state.active_field == LoginField::Password {
        active_style
    } else {
        inactive_style
    };
    let pass_block = Block::default()
        .title(" Password ")
        .borders(Borders::ALL)
        .border_style(pass_style);
    let masked_password = "*".repeat(state.password.len());
    let pass_text = Paragraph::new(masked_password)
        .style(Style::default().fg(theme.text.into()))
        .block(pass_block);
    frame.render_widget(pass_text, chunks[2]);

    // 4. Submit button / Connecting status
    let button_style = if state.active_field == LoginField::Submit {
        Style::default()
            .bg(theme.accent.into())
            .fg(theme.background.into())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(theme.text_muted.into())
            .fg(theme.text.into())
    };

    let button_label = if state.is_connecting {
        "  Connecting...  "
    } else {
        "  [ Connect & Login ]  "
    };

    let button_p = Paragraph::new(button_label)
        .alignment(Alignment::Center)
        .style(button_style);
    frame.render_widget(button_p, chunks[3]);

    // 5. Error message
    if let Some(ref err) = state.error_message {
        let err_p = Paragraph::new(format!(" Error: {}", err))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        frame.render_widget(err_p, chunks[4]);
    }
}
