use crate::core::library::settings::theme::Theme;
use ratatui::style::{Color, Modifier, Style};

pub fn p(theme: Option<&Theme>) -> Style {
    let text_color = if let Some(t) = theme {
        t.base[0x05]
    } else {
        0xffffff
    };

    Style::new().fg(Color::from_u32(text_color))
}

pub fn list_item(theme: Option<&Theme>) -> Style {
    let text_color = if let Some(t) = theme {
        t.base[0x06]
    } else {
        0xffffff
    };

    Style::new().fg(Color::from_u32(text_color))
}

pub fn h1(theme: Option<&Theme>) -> Style {
    let title_background = if let Some(t) = theme {
        t.base[0x08]
    } else {
        0xffffff
    };

    let title_color = if let Some(t) = theme {
        t.base[0x00]
    } else {
        0xffffff
    };

    Style::new()
        .bg(Color::from_u32(title_background))
        .fg(Color::from_u32(title_color))
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::UNDERLINED)
}

pub fn h2(theme: Option<&Theme>) -> Style {
    let title_color = if let Some(t) = theme {
        t.base[0x08]
    } else {
        0xffffff
    };

    Style::new()
        .fg(Color::from_u32(title_color))
        .add_modifier(Modifier::BOLD)
}

pub fn h3(theme: Option<&Theme>) -> Style {
    let title_color = if let Some(t) = theme {
        t.base[0x08]
    } else {
        0xffffff
    };

    Style::new()
        .fg(Color::from_u32(title_color))
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::ITALIC)
}

pub fn h4(theme: Option<&Theme>) -> Style {
    let title_color = if let Some(t) = theme {
        t.base[0x09]
    } else {
        0xffffff
    };

    Style::new()
        .fg(Color::from_u32(title_color))
        .add_modifier(Modifier::ITALIC)
}

pub fn h5(theme: Option<&Theme>) -> Style {
    let title_color = if let Some(t) = theme {
        t.base[0x09]
    } else {
        0xffffff
    };

    Style::new()
        .fg(Color::from_u32(title_color))
        .add_modifier(Modifier::ITALIC)
}
pub fn h6(theme: Option<&Theme>) -> Style {
    let title_color = if let Some(t) = theme {
        t.base[0x09]
    } else {
        0xffffff
    };

    Style::new()
        .fg(Color::from_u32(title_color))
        .add_modifier(Modifier::ITALIC)
}

pub fn blockquote(theme: Option<&Theme>) -> Style {
    let block_color = if let Some(t) = theme {
        t.base[0xc]
    } else {
        0xffffff
    };

    Style::new().fg(Color::from_u32(block_color))
}

pub fn code(theme: Option<&Theme>) -> Style {
    let code_color = if let Some(t) = theme {
        t.base[0xc]
    } else {
        0xffffff
    };

    let code_background = if let Some(t) = theme {
        t.base[0x0]
    } else {
        0x0
    };

    Style::new()
        .fg(Color::from_u32(code_color))
        .bg(Color::from_u32(code_background))
}

pub fn link(theme: Option<&Theme>) -> Style {
    let link_color = if let Some(t) = theme {
        t.base[0xd]
    } else {
        0xffffff
    };

    Style::new()
        .fg(Color::from_u32(link_color))
        .add_modifier(Modifier::UNDERLINED)
}

pub fn heading_meta(theme: Option<&Theme>) -> Style {
    let meta_color = if let Some(t) = theme {
        t.base[0x04]
    } else {
        0x888888
    };

    Style::new()
        .fg(Color::from_u32(meta_color))
        .add_modifier(Modifier::DIM)
}

pub fn metadata(theme: Option<&Theme>) -> Style {
    let metadata_color = if let Some(t) = theme {
        t.base[0x0a]
    } else {
        0xffffff
    };

    Style::new().fg(Color::from_u32(metadata_color))
}
