use std::{cell::RefCell, collections::VecDeque, io::stdout, rc::Rc, time::Duration};

use color_eyre::{Result, eyre};
use crossterm::{
    cursor,
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
};
#[cfg(unix)]
use libc;
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Color, Style},
    widgets::{Block, Clear, Gauge, Paragraph},
};

use crate::{
    core::{
        config::Config,
        hooks::AppHooks,
        library::feedlibrary::FeedLibrary,
        ui::{
            appscreen::{AppScreen, AppScreenEvent},
            dialog::Dialog,
            notification::{AppNotification, NotificationPriority},
        },
    },
    ui::screens::{mainscreen::MainScreen, welcomedialog::WelcomeDialog},
};

pub enum AppWorkStatus {
    None,
    Working(f32, String),
}

impl AppWorkStatus {
    pub fn is_none(&self) -> bool {
        matches!(self, AppWorkStatus::None)
    }
}

/// Linearly interpolates between two u32 colors (0x00RRGGBB format).
/// `t` ranges from 0.0 (returns `bg`) to 1.0 (returns `fg`).
fn interpolate_color(fg: u32, bg: u32, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);

    let fg_r = ((fg >> 16) & 0xFF) as f32;
    let fg_g = ((fg >> 8) & 0xFF) as f32;
    let fg_b = (fg & 0xFF) as f32;

    let bg_r = ((bg >> 16) & 0xFF) as f32;
    let bg_g = ((bg >> 8) & 0xFF) as f32;
    let bg_b = (bg & 0xFF) as f32;

    let r = (bg_r + (fg_r - bg_r) * t).round() as u8;
    let g = (bg_g + (fg_g - bg_g) * t).round() as u8;
    let b = (bg_b + (fg_b - bg_b) * t).round() as u8;

    Color::Rgb(r, g, b)
}

pub struct App {
    running: bool,
    library: Rc<RefCell<FeedLibrary>>,
    hooks: Rc<AppHooks>,
    current_state: Option<Box<dyn AppScreen>>,
    states_queue: VecDeque<Box<dyn AppScreen>>,
    dialog_queue: VecDeque<Box<dyn Dialog>>,
    active_notification: Option<AppNotification>,
    event_poll_timeout: Duration,
    config: Rc<Config>,
}

impl App {
    pub fn new(config: &Config) -> Self {
        Self {
            library: Rc::new(RefCell::new(FeedLibrary::new(&config.datapath))),
            hooks: Rc::new(config.hooks.clone().unwrap_or_default()),
            config: Rc::new(config.to_owned()),

            running: true,
            current_state: None,
            states_queue: VecDeque::<Box<dyn AppScreen>>::new(),
            dialog_queue: VecDeque::<Box<dyn Dialog>>::new(),
            active_notification: None,
            event_poll_timeout: Duration::from_millis(100),
        }
    }

    pub fn init(&mut self, mut state: Box<dyn AppScreen>) {
        state.start();
        self.current_state = Some(state);
    }

    pub fn initmain(&mut self) {
        self.init(Box::new(MainScreen::new(
            self.library.clone(),
            self.hooks.clone(),
            self.config.clone(),
        )));

        if self.library.borrow().is_empty() {
            self.open_dialog(Box::new(WelcomeDialog::new(self.library.clone())));
        }
    }

    pub fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.running {
            let theme = {
                let library = self.library.borrow();
                library.settings.get_theme().unwrap().clone()
            };

            let work_status = self.get_work_status();

            // Expire notification if its duration has elapsed
            if let Some(ref notif) = self.active_notification
                && notif.is_expired()
            {
                self.active_notification = None;
            }

            if let Some(state) = self.current_state.as_mut() {
                terminal.draw(|frame| {
                    let mainlayout =
                        Layout::vertical([Constraint::Percentage(99), Constraint::Min(3)])
                            .horizontal_margin(2)
                            .vertical_margin(1)
                            .split(frame.area());

                    state.render(frame, mainlayout[0]);

                    // Bottom status line
                    let background =
                        Block::default().style(Style::default().bg(Color::from_u32(theme.base[0])));
                    frame.render_widget(background, mainlayout[1]);

                    let title = if let Some(dialog) = self.dialog_queue.front() {
                        dialog.as_screen().get_title()
                    } else {
                        state.get_title()
                    };

                    let instructions = if let Some(dialog) = self.dialog_queue.front() {
                        dialog.as_screen().get_instructions()
                    } else {
                        state.get_instructions()
                    };

                    let instruction_width = (instructions.chars().count() as u16).min(85) + 5;

                    let statusline = Layout::horizontal([
                        Constraint::Max(25),
                        Constraint::Fill(1),
                        Constraint::Length(instruction_width),
                    ])
                    .margin(1)
                    .split(mainlayout[1]);

                    let status_text = Paragraph::new(format!("\u{f0fb1} bulletty | {title}"))
                        .style(Style::default().fg(Color::from_u32(theme.base[0x6])));

                    frame.render_widget(status_text, statusline[0]);

                    let instructions_text = Paragraph::new(instructions.to_string())
                        .style(Style::default().fg(Color::from_u32(theme.base[3])))
                        .alignment(ratatui::layout::Alignment::Right);

                    frame.render_widget(instructions_text, statusline[2]);

                    // work status / notification display
                    // Priority: High notification > Working > Low notification
                    let is_working = matches!(work_status, AppWorkStatus::Working(_, _));
                    let show_notification = match &self.active_notification {
                        Some(notif) => match notif.priority {
                            NotificationPriority::High => true,
                            NotificationPriority::Low => !is_working,
                        },
                        None => false,
                    };

                    if show_notification {
                        if let Some(ref notif) = self.active_notification {
                            let (icon, fg_color) = match notif.priority {
                                NotificationPriority::High => {
                                    ("\u{f06a}", theme.base[0x8]) // exclamation circle
                                }
                                NotificationPriority::Low => {
                                    ("\u{f0f3}", theme.base[0x9]) // bell
                                }
                            };

                            let fade = notif.fade_ratio();
                            let color = interpolate_color(fg_color, theme.base[0x0], fade);

                            let notification_text =
                                Paragraph::new(format!("{icon} {}", notif.message))
                                    .style(Style::default().fg(color))
                                    .alignment(ratatui::layout::Alignment::Center);
                            frame.render_widget(notification_text, statusline[1]);
                        }
                    } else if let AppWorkStatus::Working(percentage, description) = work_status {
                        let gauge = Gauge::default()
                            .gauge_style(
                                Style::default()
                                    .fg(Color::from_u32(theme.base[0x9]))
                                    .bg(Color::from_u32(theme.base[0x2])),
                            )
                            .percent((percentage * 100.0).round() as u16)
                            .label(&description);
                        frame.render_widget(gauge, statusline[1]);
                    }

                    // After drawing the state, needs to check if there's a dialog
                    if let Some(dialog) = self.dialog_queue.get_mut(0) {
                        let overlay = Block::default().style(
                            Style::default()
                                .bg(Color::from_u32(theme.base[2]))
                                .fg(Color::from_u32(theme.base[4])),
                        );
                        frame.render_widget(overlay, mainlayout[0]);

                        let border =
                            Block::new().style(Style::new().bg(Color::from_u32(theme.base[1])));
                        let block =
                            Block::new().style(Style::new().bg(Color::from_u32(theme.base[0])));

                        let area = popup_area(mainlayout[0], dialog.get_size());
                        let inner_area = area.inner(Margin::new(2, 1));

                        frame.render_widget(Clear, area);
                        frame.render_widget(border, area);
                        frame.render_widget(block, inner_area);

                        dialog.as_screen_mut().render(frame, inner_area);
                    }
                })?;

                // Checking the dialog or the state events
                let event_available =
                    crossterm::event::poll(self.event_poll_timeout).unwrap_or(true);
                if event_available {
                    let mut terminal_event = crossterm::event::read()?;

                    #[cfg(unix)]
                    if let Event::Key(KeyEvent {
                        code: KeyCode::Char('z'),
                        modifiers: KeyModifiers::CONTROL,
                        kind: KeyEventKind::Press,
                        ..
                    }) = terminal_event
                    {
                        ratatui::restore();
                        let _ = execute!(stdout(), cursor::Show);
                        unsafe {
                            libc::raise(libc::SIGTSTP);
                        }
                        terminal = ratatui::init();
                        continue;
                    }

                    if let Event::Mouse(event) = terminal_event {
                        let key_code = match event.kind {
                            crossterm::event::MouseEventKind::ScrollUp => Some(KeyCode::Up),
                            crossterm::event::MouseEventKind::ScrollDown => Some(KeyCode::Down),
                            _ => None,
                        };

                        if let Some(key_code) = key_code {
                            terminal_event = Event::Key(KeyEvent::new(key_code, KeyModifiers::NONE))
                        }
                    }

                    let screen = if let Some(dialog) = self.dialog_queue.get_mut(0) {
                        dialog.as_screen_mut()
                    } else {
                        state.as_mut()
                    };

                    match screen.handle_event(terminal_event)? {
                        AppScreenEvent::None => {}

                        AppScreenEvent::ChangeState(app_state) => {
                            self.change_state(app_state);
                        }
                        AppScreenEvent::ExitState => {
                            self.exit_state();
                        }

                        AppScreenEvent::OpenDialog(app_state) => {
                            self.open_dialog(app_state);
                        }
                        AppScreenEvent::CloseDialog => {
                            self.close_current_dialog();
                        }

                        AppScreenEvent::Notify(notification) => {
                            self.active_notification = Some(notification);
                        }

                        AppScreenEvent::ExitApp => {
                            self.running = false;
                        }
                    }
                };
            } else {
                self.running = false;
                return Err(eyre::eyre!("No current AppState"));
            }
        }

        if let Some(state) = self.current_state.as_mut() {
            state.quit();
        }

        Ok(())
    }

    fn change_state(&mut self, mut new_state: Box<dyn AppScreen>) {
        if let Some(mut state) = self.current_state.take() {
            state.pause();
            self.states_queue.push_back(state);
        }

        new_state.start();
        self.current_state = Some(new_state);
    }

    fn exit_state(&mut self) {
        if let Some(mut state) = self.current_state.take() {
            state.quit();
        }

        if !self.states_queue.is_empty() {
            if let Some(mut state) = self.states_queue.pop_back() {
                state.unpause();
                self.current_state = Some(state);
            } else {
                self.running = false;
            }
        } else {
            self.running = false;
        }
    }

    fn get_work_status(&self) -> AppWorkStatus {
        if let Some(state) = self.current_state.as_ref() {
            let status = state.get_work_status();
            if !status.is_none() {
                return status;
            }
        }

        self.states_queue
            .iter()
            .map(|state| state.get_work_status())
            .find(|state| !state.is_none())
            .unwrap_or(AppWorkStatus::None)
    }

    fn open_dialog(&mut self, mut dialog_state: Box<dyn Dialog>) {
        if let Some(state) = self.current_state.as_mut() {
            state.pause();
        }

        dialog_state.as_screen_mut().start();
        self.dialog_queue.push_back(dialog_state);
    }

    fn close_current_dialog(&mut self) {
        if let Some(mut state) = self.dialog_queue.pop_back() {
            state.as_screen_mut().quit();
        }
    }
}

fn popup_area(area: Rect, size: Rect) -> Rect {
    let mut vertical = Layout::vertical([Constraint::Percentage(size.height)]).flex(Flex::Center);
    let mut horizontal =
        Layout::horizontal([Constraint::Percentage(size.width)]).flex(Flex::Center);

    if size.x + size.y > 0 {
        vertical = Layout::vertical([Constraint::Length(size.y)]).flex(Flex::Center);
        horizontal = Layout::horizontal([Constraint::Length(size.x)]).flex(Flex::Center);
    }

    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);

    area
}
