use std::{cell::RefCell, rc::Rc};

use color_eyre::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{
    Block, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};
use tracing::error;
use unicode_width::UnicodeWidthStr;

use crate::app::AppWorkStatus;
use crate::core::ui::notification::{AppNotification, NotificationPriority};
use crate::core::{
    feed::feedentry::FeedEntry,
    hooks::AppHooks,
    library::feedlibrary::FeedLibrary,
    ui::{
        appscreen::{AppScreen, AppScreenEvent},
        instructiondetails::{InstructionCategory, InstructionDetail, ScreenInstructions},
    },
};
use crate::ui::screens::themedialog::ThemeDialog;
use crate::ui::screens::urldialog::UrlDialog;
use crate::ui::tools::tuimarkdown;

use super::helpdialog::HelpDialog;

pub struct ReaderScreen {
    library: Rc<RefCell<FeedLibrary>>,
    entries: Vec<FeedEntry>,
    current_index: usize,
    scroll: usize,
    scrollmax: usize,
    viewport_height: usize,
    hooks: Rc<AppHooks>,
}

impl ReaderScreen {
    pub fn new(
        library: Rc<RefCell<FeedLibrary>>,
        entries: Vec<FeedEntry>,
        current_index: usize,
        hooks: Rc<AppHooks>,
    ) -> ReaderScreen {
        ReaderScreen {
            library,
            entries,
            current_index,
            scroll: 0,
            scrollmax: 1,
            viewport_height: 24,
            hooks,
        }
    }

    pub fn scrollup(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }

    pub fn scrolldown(&mut self) {
        self.scroll = std::cmp::min(self.scroll + 1, self.scrollmax);
    }

    pub fn scroll_half_down(&mut self) {
        let half = (self.viewport_height / 2).max(1);
        self.scroll = (self.scroll + half).min(self.scrollmax);
    }

    pub fn scroll_half_up(&mut self) {
        let half = (self.viewport_height / 2).max(1);
        self.scroll = self.scroll.saturating_sub(half);
    }

    pub fn next_entry(&mut self) {
        if self.current_index < self.entries.len().saturating_sub(1) {
            self.current_index += 1;
            self.scroll = 0;
            self.library
                .borrow_mut()
                .set_entry_seen(&self.entries[self.current_index]);
        }
    }

    pub fn previous_entry(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
            self.scroll = 0;
            self.library
                .borrow_mut()
                .set_entry_seen(&self.entries[self.current_index]);
        }
    }

    fn open_external_url(&self, url: &str) -> Result<AppScreenEvent> {
        if self.hooks.run_open_link(url) {
            return Ok(AppScreenEvent::Notify(AppNotification::new(
                "Link opened externally with user hook",
                NotificationPriority::Low,
            )));
        }

        match open::that_detached(url) {
            Ok(_) => Ok(AppScreenEvent::Notify(AppNotification::new(
                "Link opened externally",
                NotificationPriority::Low,
            ))),
            Err(_) => {
                error!("Couldn't invoke system browser");
                Ok(AppScreenEvent::OpenDialog(Box::new(UrlDialog::new(
                    url.to_string(),
                ))))
            }
        }
    }

    fn increase_reader_width(&mut self) -> color_eyre::Result<()> {
        let mut l = self.library.borrow_mut();
        l.settings.appearance.reader_width = l
            .settings
            .appearance
            .reader_width
            .saturating_add(2)
            .min(100);
        l.settings.appearance.save()
    }

    fn decrease_reader_width(&mut self) -> color_eyre::Result<()> {
        let mut l = self.library.borrow_mut();
        l.settings.appearance.reader_width = l
            .settings
            .appearance
            .reader_width
            .saturating_sub(2)
            .min(100);
        l.settings.appearance.save()
    }
}

impl AppScreen for ReaderScreen {
    fn start(&mut self) {}

    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let theme = {
            let library = self.library.borrow();
            library.settings.get_theme().unwrap().clone()
        };

        let block = Block::default()
            .style(Style::default().bg(Color::from_u32(theme.base[1])))
            .padding(Padding::new(3, 3, 3, 3));

        frame.render_widget(block, area);

        let width = self.library.borrow().settings.appearance.reader_width;

        let sizelayout = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Percentage(width),
            Constraint::Max(3),
            Constraint::Fill(1),
        ])
        .margin(2)
        .split(area);

        let contentlayout = Layout::vertical([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Date
            Constraint::Length(2), // URL
            Constraint::Fill(1),   // Content
        ])
        .split(sizelayout[1]);

        let current_entry = &self.entries[self.current_index];

        // Title
        let title = Paragraph::new(current_entry.title.as_str())
            .style(Style::new().fg(Color::from_u32(theme.base[0x8])))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        frame.render_widget(title, contentlayout[0]);

        // Date
        let date = Paragraph::new(format!(
            "\u{f0520} {} | \u{f09e} {}",
            current_entry
                .date
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d"),
            current_entry.author
        ))
        .style(Style::new().fg(Color::from_u32(theme.base[3])))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

        frame.render_widget(date, contentlayout[1]);

        // URL
        let date = Paragraph::new(current_entry.url.to_string())
            .style(
                Style::new().fg(Color::from_u32(theme.base[0xd])), // .add_modifier(Modifier::UNDERLINED),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        frame.render_widget(date, contentlayout[2]);

        // Content
        let text = tuimarkdown::from_str(&current_entry.text, Some(theme.clone()));
        let textheight = text.height() as usize;

        // This is a workaround to get more or less the amount of wrapped lines, to be used on the
        // scrollbar
        let mut wrapped_lines = 0;
        for line in text.lines.iter() {
            let content: String = line
                .spans
                .iter()
                .map(|span| span.content.to_string())
                .collect();
            let line_width = UnicodeWidthStr::width(content.as_str());
            let wrapped = line_width.div_ceil(contentlayout[3].width as usize);
            wrapped_lines += wrapped - wrapped.min(1);
        }

        let scrollheight = textheight + (wrapped_lines as f32 * 1.06) as usize + 4;
        self.scrollmax = scrollheight - (contentlayout[3].height as usize).min(scrollheight);
        self.viewport_height = contentlayout[3].height as usize;

        // Content Paragraph component
        let paragraph = Paragraph::new(text)
            .scroll((self.scroll as u16, 0))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, contentlayout[3]);

        // Scrollbar
        let mut scrollbarstate = ScrollbarState::new(self.scrollmax).position(self.scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight).style(
            Style::new()
                .fg(Color::from_u32(theme.base[3]))
                .bg(Color::from_u32(theme.base[1])),
        );
        frame.render_stateful_widget(scrollbar, sizelayout[2], &mut scrollbarstate);
    }

    fn handle_event(&mut self, event: Event) -> Result<AppScreenEvent> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_keypress(key),
            Event::Mouse(_) => Ok(AppScreenEvent::None),
            Event::Resize(_, _) => Ok(AppScreenEvent::None),
            _ => Ok(AppScreenEvent::None),
        }
    }

    fn handle_keypress(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> color_eyre::eyre::Result<AppScreenEvent> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                Ok(AppScreenEvent::ExitState)
            }
            (_, KeyCode::Down | KeyCode::Char('j')) => {
                self.scrolldown();
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Up | KeyCode::Char('k')) => {
                self.scrollup();
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Home | KeyCode::Char('g')) => {
                self.scroll = 0;
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::End | KeyCode::Char('G')) => {
                self.scroll = self.scrollmax;
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Char('o')) => {
                self.open_external_url(&self.entries[self.current_index].url)
            }
            (_, KeyCode::Char('n')) => {
                self.next_entry();
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Char('p')) => {
                self.previous_entry();
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Char('>')) => {
                self.increase_reader_width()?;
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Char('<')) => {
                self.decrease_reader_width()?;
                Ok(AppScreenEvent::None)
            }
            (KeyModifiers::CONTROL, KeyCode::Char('d') | KeyCode::Char('D')) => {
                self.scroll_half_down();
                Ok(AppScreenEvent::None)
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u') | KeyCode::Char('U')) => {
                self.scroll_half_up();
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Char('t')) => Ok(AppScreenEvent::OpenDialog(Box::new(ThemeDialog::new(
                self.library.clone(),
            )))),
            (_, KeyCode::Char('?')) => Ok(AppScreenEvent::OpenDialog(Box::new(HelpDialog::new(
                self.library.clone(),
                self.get_full_instructions(),
            )))),
            _ => Ok(AppScreenEvent::None),
        }
    }

    fn pause(&mut self) {}

    fn unpause(&mut self) {}

    fn quit(&mut self) {}

    fn get_title(&self) -> String {
        String::from("Reader")
    }

    fn get_instructions(&self) -> String {
        String::from("?: Help | j/k/↓/↑: scroll | n/p: next/prev | o: open | Esc/q: leave")
    }

    fn get_work_status(&self) -> AppWorkStatus {
        AppWorkStatus::None
    }

    fn get_full_instructions(&self) -> ScreenInstructions {
        ScreenInstructions::new(vec![
            InstructionCategory::new(
                "Navigation",
                vec![
                    InstructionDetail::new("j/k/↓/↑", "scroll"),
                    InstructionDetail::new("Ctrl+d/u", "scroll half page down/up"),
                    InstructionDetail::new("g/G", "go to beginning or end of file"),
                    InstructionDetail::new("</>", "change reader width"),
                    InstructionDetail::new("n/p", "next/previous entry"),
                ],
            ),
            InstructionCategory::new(
                "Actions",
                vec![InstructionDetail::new("o", "open externally")],
            ),
            InstructionCategory::new(
                "App",
                vec![
                    InstructionDetail::new("t", "open theme picker"),
                    InstructionDetail::new("Esc/q", "leave"),
                ],
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::library::feedlibrary::FeedLibrary;

    fn create_test_entries() -> Vec<FeedEntry> {
        vec![
            FeedEntry {
                title: "Entry 1".to_string(),
                ..Default::default()
            },
            FeedEntry {
                title: "Entry 2".to_string(),
                ..Default::default()
            },
            FeedEntry {
                title: "Entry 3".to_string(),
                ..Default::default()
            },
        ]
    }

    #[test]
    fn test_navigation() {
        let (library, _temp_dir) = FeedLibrary::new_for_test();
        let entries = create_test_entries();
        let mut reader_screen = ReaderScreen::new(
            Rc::new(RefCell::new(library)),
            entries,
            0,
            Rc::new(AppHooks::default()),
        );

        // Test next_entry
        assert_eq!(reader_screen.current_index, 0);
        reader_screen.next_entry();
        assert_eq!(reader_screen.current_index, 1);
        reader_screen.next_entry();
        assert_eq!(reader_screen.current_index, 2);
        // Should not go past the last entry
        reader_screen.next_entry();
        assert_eq!(reader_screen.current_index, 2);

        // Test previous_entry
        reader_screen.previous_entry();
        assert_eq!(reader_screen.current_index, 1);
        reader_screen.previous_entry();
        assert_eq!(reader_screen.current_index, 0);
        // Should not go before the first entry
        reader_screen.previous_entry();
        assert_eq!(reader_screen.current_index, 0);
    }
}
