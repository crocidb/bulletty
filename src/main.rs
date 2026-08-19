pub mod app;
pub mod cli;
pub mod core;
mod dirs;
pub mod logging;
pub mod mainui;
pub mod ui;

use clap::Parser;
use color_eyre::eyre::Context;

use crate::{
    core::config::{Config, ConfigStore},
    dirs::Directories,
};

pub fn run() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let dirs = Directories::new().wrap_err("Failed to construct base directories")?;

    let _guard = logging::init(dirs.log());

    let config_store = ConfigStore::new(dirs.config());
    let mut config = config_store.get_or_create(|| Config {
        datapath: dirs.default_data().into(),
        hooks: None,
        tui_auto_update: Some(true),
        parallel_feed_updates: Some(true),
        hide_unread_count: Some(false),
    })?;

    let cli = cli::Cli::parse();

    if cli.no_hooks {
        config.hooks = None;
    }

    if cli.command.is_none() {
        mainui::run_main_ui(&config)
    } else {
        cli::run_main_cli(cli, &dirs, &mut config, &config_store)
    }
}

fn main() -> color_eyre::Result<()> {
    run()
}
