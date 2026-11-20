use std::num::NonZeroU16;

use ratatui::buffer::{Buffer, CellDiffOption, CellWidth};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::Text;
use ratatui::widgets::{Paragraph, Widget, Wrap};

const SENTINEL_SYMBOL: &str = "\0";

#[derive(Debug, Clone)]
pub struct Hyperlink<'a> {
    text: Text<'a>,
    url: String,
    id: Option<String>,
    style: Style,
    alignment: Alignment,
    wrap: Option<Wrap>,
}

impl<'a> Hyperlink<'a> {
    pub fn new(text: impl Into<Text<'a>>, url: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            url: url.into(),
            id: None,
            style: Style::default(),
            alignment: Alignment::Left,
            wrap: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub const fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    #[must_use]
    pub const fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    #[must_use]
    pub const fn wrap(mut self, wrap: Wrap) -> Self {
        self.wrap = Some(wrap);
        self
    }
}

fn sanitize_url(url: &str) -> String {
    url.chars().filter(|c| !c.is_control()).collect()
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|c| !c.is_control() && *c != ':' && *c != ';')
        .collect()
}

fn link_id(url: &str, area: Rect) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    let prime: u64 = 0x100000001b3;

    for byte in url
        .as_bytes()
        .iter()
        .copied()
        .chain(area.x.to_le_bytes())
        .chain(area.y.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(prime);
    }

    format!("{hash:016x}")
}

impl Widget for Hyperlink<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Widget::render(&self, area, buf);
    }
}

impl Widget for &Hyperlink<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }

        let mut scratch = Buffer::empty(area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let mut cell = buf[(x, y)].clone();
                cell.set_symbol(SENTINEL_SYMBOL);
                cell.set_diff_option(CellDiffOption::None);
                scratch[(x, y)] = cell;
            }
        }

        let styled_text = self.text.clone().patch_style(self.style);
        let mut paragraph = Paragraph::new(styled_text).alignment(self.alignment);
        if let Some(wrap) = self.wrap {
            paragraph = paragraph.wrap(wrap);
        }
        paragraph.render(area, &mut scratch);

        let sanitized_url = sanitize_url(&self.url);
        let id = if sanitized_url.is_empty() {
            String::new()
        } else {
            self.id
                .as_deref()
                .map(sanitize_id)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| link_id(&sanitized_url, area))
        };

        for y in area.top()..area.bottom() {
            let mut x = area.left();
            while x < area.right() {
                let scratch_cell = &scratch[(x, y)];
                let symbol = scratch_cell.symbol();
                if symbol == SENTINEL_SYMBOL {
                    x += 1;
                    continue;
                }

                let width = symbol.cell_width().max(1);
                let mut target = scratch_cell.clone();

                if !sanitized_url.is_empty() {
                    let osc8 =
                        format!("\x1b]8;id={id};{sanitized_url}\x1b\\{symbol}\x1b]8;;\x1b\\");
                    target.set_symbol(&osc8);
                    if let Some(forced_width) = NonZeroU16::new(width) {
                        target.set_diff_option(CellDiffOption::ForcedWidth(forced_width));
                    }
                }

                buf[(x, y)] = target;

                for dx in 1..width {
                    if x + dx < area.right() {
                        buf[(x + dx, y)]
                            .set_symbol(" ")
                            .set_diff_option(CellDiffOption::None);
                    }
                }

                x = x.saturating_add(width);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};
    use ratatui::widgets::Block;

    #[test]
    fn renders_single_line_osc8_hyperlink() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);

        let url = "https://example.com";
        let widget = Hyperlink::new("link", url).style(Style::new().fg(Color::Cyan));
        widget.render(area, &mut buf);

        let expected_id = link_id(url, area);
        for (i, ch) in "link".chars().enumerate() {
            let cell = &buf[(i as u16, 0)];
            assert_eq!(
                cell.symbol(),
                format!("\x1b]8;id={expected_id};{url}\x1b\\{ch}\x1b]8;;\x1b\\")
            );
            assert_eq!(cell.fg, Color::Cyan);
            assert_eq!(
                cell.diff_option,
                CellDiffOption::ForcedWidth(NonZeroU16::new(1).unwrap())
            );
        }

        for x in 4..10 {
            let cell = &buf[(x, 0)];
            assert_eq!(cell.symbol(), " ");
            assert_eq!(cell.diff_option, CellDiffOption::None);
        }
    }

    #[test]
    fn centers_text_without_linking_padding_and_preserves_bg() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        Block::default()
            .style(Style::new().bg(Color::Rgb(10, 20, 30)))
            .render(area, &mut buf);

        let url = "https://ratatui.rs";
        let widget = Hyperlink::new("hi u", url)
            .style(
                Style::new()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::UNDERLINED),
            )
            .alignment(Alignment::Center);
        widget.render(area, &mut buf);

        let expected_id = link_id(url, area);
        for x in 0..3 {
            assert_eq!(buf[(x, 0)].symbol(), " ");
            assert_eq!(buf[(x, 0)].bg, Color::Rgb(10, 20, 30));
            assert!(!buf[(x, 0)].modifier.contains(Modifier::UNDERLINED));
            assert_eq!(buf[(x, 0)].diff_option, CellDiffOption::None);
        }

        for (i, ch) in "hi u".chars().enumerate() {
            let cell = &buf[(3 + i as u16, 0)];
            assert_eq!(
                cell.symbol(),
                format!("\x1b]8;id={expected_id};{url}\x1b\\{ch}\x1b]8;;\x1b\\")
            );
            assert_eq!(cell.fg, Color::Yellow);
            assert_eq!(cell.bg, Color::Rgb(10, 20, 30));
            assert!(cell.modifier.contains(Modifier::UNDERLINED));
        }

        for x in 7..10 {
            assert_eq!(buf[(x, 0)].symbol(), " ");
            assert_eq!(buf[(x, 0)].bg, Color::Rgb(10, 20, 30));
            assert_eq!(buf[(x, 0)].diff_option, CellDiffOption::None);
        }
    }

    #[test]
    fn wrapped_lines_share_the_same_osc8_id() {
        let area = Rect::new(0, 0, 10, 2);
        let mut buf = Buffer::empty(area);

        let url = "https://example.com/long";
        let widget = Hyperlink::new(url, url)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        widget.render(area, &mut buf);

        let expected_id = link_id(url, area);
        let prefix = format!("\x1b]8;id={expected_id};{url}\x1b\\");

        assert!(buf[(0, 0)].symbol().starts_with(&prefix));
        assert!(
            (0..10).any(|x| buf[(x, 1)].symbol().starts_with(&prefix)),
            "second wrapped line should contain OSC 8 hyperlink cells"
        );
    }

    #[test]
    fn handles_wide_unicode_characters() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);

        let url = "https://example.com";
        let widget = Hyperlink::new("🔗a", url);
        widget.render(area, &mut buf);

        let expected_id = link_id(url, area);
        assert_eq!(
            buf[(0, 0)].symbol(),
            format!("\x1b]8;id={expected_id};{url}\x1b\\🔗\x1b]8;;\x1b\\")
        );
        assert_eq!(
            buf[(0, 0)].diff_option,
            CellDiffOption::ForcedWidth(NonZeroU16::new(2).unwrap())
        );
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].diff_option, CellDiffOption::None);

        assert_eq!(
            buf[(2, 0)].symbol(),
            format!("\x1b]8;id={expected_id};{url}\x1b\\a\x1b]8;;\x1b\\")
        );
        assert_eq!(
            buf[(2, 0)].diff_option,
            CellDiffOption::ForcedWidth(NonZeroU16::new(1).unwrap())
        );
    }

    #[test]
    fn empty_url_renders_plain_text_without_osc8() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);

        Hyperlink::new("plain", "").render(area, &mut buf);

        for (i, ch) in "plain".chars().enumerate() {
            let cell = &buf[(i as u16, 0)];
            assert_eq!(cell.symbol(), ch.to_string());
            assert_eq!(cell.diff_option, CellDiffOption::None);
        }
    }

    #[test]
    fn sanitizes_control_characters_from_url() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);

        let malicious = "https://example.com\x1b\\injected\x07\n";
        Hyperlink::new("x", malicious).render(area, &mut buf);

        let clean = "https://example.com\\injected";
        let expected_id = link_id(clean, area);
        assert_eq!(
            buf[(0, 0)].symbol(),
            format!("\x1b]8;id={expected_id};{clean}\x1b\\x\x1b]8;;\x1b\\")
        );
    }

    #[test]
    fn buffer_diff_is_idempotent_and_handles_partial_overwrite() {
        let area = Rect::new(0, 0, 10, 1);
        let url = "https://example.com";

        let mut prev = Buffer::empty(area);
        Hyperlink::new("0123456789", url).render(area, &mut prev);

        // Rendering the same link again yields zero diff.
        let mut same = Buffer::empty(area);
        Hyperlink::new("0123456789", url).render(area, &mut same);
        assert_eq!(prev.diff(&same), []);

        // Overlapping content in the middle (e.g., a dialog) must be emitted cleanly.
        let mut overlapped = same.clone();
        overlapped.set_string(3, 0, "BOX", Style::default());
        let diff = prev.diff(&overlapped);
        assert_eq!(diff.len(), 3);
        assert_eq!(diff[0].0, 3);
        assert_eq!(diff[0].2.symbol(), "B");
        assert_eq!(diff[1].0, 4);
        assert_eq!(diff[1].2.symbol(), "O");
        assert_eq!(diff[2].0, 5);
        assert_eq!(diff[2].2.symbol(), "X");

        // Clearing the link emits all 10 cells with no ghost characters left behind.
        let cleared = Buffer::empty(area);
        let clear_diff = prev.diff(&cleared);
        assert_eq!(clear_diff.len(), 10);
    }

    #[test]
    fn explicit_id_is_used_and_sanitized() {
        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);
        let url = "https://example.com";

        Hyperlink::new("a", url)
            .id("grp:1;bad\x1b")
            .render(area, &mut buf);

        assert_eq!(
            buf[(0, 0)].symbol(),
            format!("\x1b]8;id=grp1bad;{url}\x1b\\a\x1b]8;;\x1b\\")
        );
    }
}
