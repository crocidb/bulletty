use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;
use etcetera::BaseStrategy;

use crate::core::defs::{CONFIG_PATH, DATA_DIR, DATA_THEMES_DIR, LOG_BASE_DIR, LOG_SUBDIR};

pub struct Directories {
    log: PathBuf,
    default_data: PathBuf,
    config: PathBuf,
    themes: PathBuf,
}

impl Directories {
    pub fn new() -> Result<Self> {
        let current_strategy = etcetera::choose_base_strategy()?;

        Ok(Directories {
            log: current_strategy
                .state_dir() // Exists on Linux/macOS only
                .unwrap_or_else(|| current_strategy.cache_dir())
                .join(LOG_BASE_DIR)
                .join(LOG_SUBDIR),
            default_data: current_strategy.data_dir().join(DATA_DIR),
            config: current_strategy.config_dir().join(CONFIG_PATH),
            themes: current_strategy
                .data_dir()
                .join(DATA_DIR)
                .join(DATA_THEMES_DIR),
        })
    }

    pub fn log(&self) -> &Path {
        &self.log
    }

    pub fn default_data(&self) -> &Path {
        &self.default_data
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn themes(&self) -> &Path {
        &self.themes
    }
}
