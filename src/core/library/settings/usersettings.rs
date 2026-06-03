use std::{collections::HashMap, ffi::OsString, fs, path::Path};

use crate::core::{
    defs::DATA_THEMES_DIR,
    library::settings::{appearance::Appearance, theme::Theme, themedata},
};

pub struct UserSettings {
    pub appearance: Appearance,
    themes: HashMap<String, Theme>,
}

impl UserSettings {
    pub fn new(datapath: &Path) -> color_eyre::Result<Self> {
        let themes_from_library = UserSettings::get_theme_files(datapath);
        let embedded_themes = themedata::get_themes();
        let themes = embedded_themes
            .into_iter()
            .chain(themes_from_library)
            .collect::<HashMap<String, Theme>>();

        Ok(Self {
            appearance: Appearance::new(datapath)?,
            themes,
        })
    }

    pub fn get_theme(&self) -> Option<&Theme> {
        if let Some(theme) = self.themes.get(&self.appearance.theme) {
            return Some(theme);
        }

        if let Some((_, value)) = self.themes.iter().next() {
            Some(value)
        } else {
            None
        }
    }

    pub fn get_theme_list(&self) -> Vec<String> {
        self.themes.keys().map(|t| t.to_string()).collect()
    }

    fn get_theme_files(datapath: &Path) -> HashMap<String, Theme> {
        let theme_file_ext = OsString::from("toml");
        let themes_path = Path::join(datapath, DATA_THEMES_DIR);
        let mut themes: Vec<Theme> = Vec::new();

        if let Ok(entries) = fs::read_dir(themes_path) {
            for entry in entries {
                if let Ok(entry) = entry
                    && let path = entry.path()
                    && path.is_file()
                    && path.extension() == Some(&theme_file_ext)
                    && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(mut theme) = toml::from_str::<Theme>(&content)
                {
                    theme.base[0x0] = u32::from_str_radix(&theme.base00, 16).unwrap();
                    theme.base[0x1] = u32::from_str_radix(&theme.base01, 16).unwrap();
                    theme.base[0x2] = u32::from_str_radix(&theme.base02, 16).unwrap();
                    theme.base[0x3] = u32::from_str_radix(&theme.base03, 16).unwrap();
                    theme.base[0x4] = u32::from_str_radix(&theme.base04, 16).unwrap();
                    theme.base[0x5] = u32::from_str_radix(&theme.base05, 16).unwrap();
                    theme.base[0x6] = u32::from_str_radix(&theme.base06, 16).unwrap();
                    theme.base[0x7] = u32::from_str_radix(&theme.base07, 16).unwrap();
                    theme.base[0x8] = u32::from_str_radix(&theme.base08, 16).unwrap();
                    theme.base[0x9] = u32::from_str_radix(&theme.base09, 16).unwrap();
                    theme.base[0xa] = u32::from_str_radix(&theme.base0A, 16).unwrap();
                    theme.base[0xb] = u32::from_str_radix(&theme.base0B, 16).unwrap();
                    theme.base[0xc] = u32::from_str_radix(&theme.base0C, 16).unwrap();
                    theme.base[0xd] = u32::from_str_radix(&theme.base0D, 16).unwrap();
                    theme.base[0xe] = u32::from_str_radix(&theme.base0E, 16).unwrap();
                    theme.base[0xf] = u32::from_str_radix(&theme.base0F, 16).unwrap();
                    themes.push(theme);
                }
            }
        }

        themes
            .iter()
            .map(|theme| (theme.scheme.clone(), theme.clone()))
            .collect::<HashMap<String, Theme>>()
    }
}
