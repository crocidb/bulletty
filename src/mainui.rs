use std::io::stdout;
use tracing::info;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
};

use crate::{app, core::config::Config};

pub fn run_main_ui(config: &Config) -> color_eyre::Result<()> {
    info!("Initializing UI");

    if let Some(hooks) = &config.hooks {
        hooks.run_before_tui();
    }

    let terminal = ratatui::init();

    execute!(stdout(), EnableMouseCapture).ok();

    // update panic hook to DisableMouseCapture on panic
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        execute!(stdout(), DisableMouseCapture).ok();
        ratatui::restore();
        hook(info);
    }));

    let mut app = app::App::new(config);
    app.initmain();
    let result = app.run(terminal);
    execute!(stdout(), DisableMouseCapture).ok();
    ratatui::restore();

    if let Some(hooks) = &config.hooks {
        hooks.run_after_tui();
    }

    result
}
