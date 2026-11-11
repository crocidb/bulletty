use std::{cell::RefCell, rc::Rc};

use color_eyre::eyre::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{
    Block, List, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};

use crate::app::AppWorkStatus;

use crate::core::library::feedlibrary::FeedLibrary;
use crate::core::ui::appscreen::{AppScreen, AppScreenEvent};
use crate::core::ui::dialog::Dialog;
use crate::core::ui::instructiondetails::ScreenInstructions;
use crate::ui::states::themestate::ThemeState;

pub struct ThemeDialog {
    library: Rc<RefCell<FeedLibrary>>,
    state: ThemeState,
}

impl ThemeDialog {
    pub fn new(library: Rc<RefCell<FeedLibrary>>) -> Self {
        Self {
            library,
            state: ThemeState::default(),
        }
    }
}

impl Dialog for ThemeDialog {
    fn get_size(&self) -> ratatui::prelude::Rect {
        Rect::new(50, 20, 0, 0)
    }

    fn as_screen(&self) -> &dyn AppScreen {
        self
    }

    fn as_screen_mut(&mut self) -> &mut dyn AppScreen {
        self
    }
}

impl AppScreen for ThemeDialog {
    fn start(&mut self) {}

    fn quit(&mut self) {}

    fn pause(&mut self) {}

    fn unpause(&mut self) {}

    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let theme = {
            let library = self.library.borrow();
            library.settings.get_theme().unwrap().clone()
        };

        let contentlayout = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
            .split(area.inner(Margin::new(2, 1)));

        let title = Paragraph::new(self.get_title())
            .style(Style::new().fg(Color::from_u32(theme.base[0x8])))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        // List
        let chunks = Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
            .split(contentlayout[1]);

        self.state.update(&self.library.borrow());

        let themelist = List::new(self.state.get_items())
            .block(
                Block::default()
                    .style(
                        Style::default()
                            .fg(Color::from_u32(theme.base[0x5]))
                            .bg(Color::from_u32(theme.base[0x1])),
                    )
                    .padding(Padding::new(1, 1, 1, 1)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::from_u32(theme.base[0x2]))
                    .bg(Color::from_u32(theme.base[0x8])),
            );

        frame.render_widget(title, contentlayout[0]);
        let mut themeliststate = self.state.state;
        frame.render_stateful_widget(themelist, chunks[0], &mut themeliststate);
        self.state.state = themeliststate;

        // Scrollbar
        let mut scrollbarstate =
            ScrollbarState::new(self.state.scroll_max()).position(self.state.scroll());
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight).style(
            Style::new()
                .fg(Color::from_u32(theme.base[3]))
                .bg(Color::from_u32(theme.base[2])),
        );
        frame.render_stateful_widget(scrollbar, chunks[1], &mut scrollbarstate);
    }

    fn handle_event(&mut self, event: Event) -> Result<AppScreenEvent> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_keypress(key),
            Event::Mouse(_) => Ok(AppScreenEvent::None),
            Event::Resize(_, _) => Ok(AppScreenEvent::None),
            _ => Ok(AppScreenEvent::None),
        }
    }

    fn handle_keypress(&mut self, key: KeyEvent) -> Result<AppScreenEvent> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter)
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                Ok(AppScreenEvent::CloseDialog)
            }
            (_, KeyCode::Down | KeyCode::Char('j')) => {
                self.state.select_next();
                let selected = self.state.get_selected();
                self.library.borrow_mut().settings.appearance.theme = selected.unwrap();
                self.library.borrow_mut().settings.appearance.save()?;
                Ok(AppScreenEvent::None)
            }
            (_, KeyCode::Up | KeyCode::Char('k')) => {
                self.state.select_previous();
                let selected = self.state.get_selected();
                self.library.borrow_mut().settings.appearance.theme = selected.unwrap();
                self.library.borrow_mut().settings.appearance.save()?;
                Ok(AppScreenEvent::None)
            }
            _ => Ok(AppScreenEvent::None),
        }
    }

    fn get_work_status(&self) -> AppWorkStatus {
        AppWorkStatus::None
    }

    fn get_title(&self) -> String {
        String::from("Theme")
    }

    fn get_instructions(&self) -> String {
        String::from("j/k: select theme | Esc/q: close url")
    }

    fn get_full_instructions(&self) -> ScreenInstructions {
        ScreenInstructions::empty()
    }
}
