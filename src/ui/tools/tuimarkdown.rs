//! A simple markdown renderer widget for Ratatui.
//! Originally written by joshka
//! https://github.com/joshka/tui-markdown

use std::collections::VecDeque;
use std::mem;
use std::sync::LazyLock;
use std::vec;

use ansi_to_tui::IntoText;
use itertools::Itertools;
use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, StyledGrapheme, Text};
use ratatui::widgets::Widget;
use syntect::{
    easy::HighlightLines,
    highlighting::ThemeSet,
    parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder},
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};
use tracing::{debug, instrument, warn};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::core::library::settings::theme::Theme;
use crate::ui::tools::styles;
use crate::ui::widgets::hyperlink::Hyperlink;

/// An OSC 8 link target attached to a markdown span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    pub url: String,
    pub id: String,
}

/// A parsed markdown document holding both the styled Ratatui [`Text`] and
/// per-span hyperlink metadata so links can be rendered via [`Hyperlink`].
#[derive(Debug, Clone)]
pub struct MarkdownDocument<'a> {
    pub text: Text<'a>,
    line_links: Vec<Vec<Option<LinkTarget>>>,
}

#[derive(Debug, Clone)]
struct LinkedGrapheme<'a> {
    grapheme: StyledGrapheme<'a>,
    link: Option<&'a LinkTarget>,
}

/// A word-wrapped markdown document ready to be rendered as a scrollable
/// Ratatui [`Widget`] with clickable OSC 8 [`Hyperlink`] spans.
#[derive(Debug, Clone)]
pub struct WrappedMarkdown<'a> {
    lines: Vec<Vec<LinkedGrapheme<'a>>>,
    scroll: u16,
}

impl<'a> MarkdownDocument<'a> {
    /// Word-wraps the document to `max_width` columns (matching Ratatui's
    /// `Wrap { trim: true }` behavior) while preserving per-grapheme link metadata.
    pub fn wrap(&self, max_width: u16) -> WrappedMarkdown<'_> {
        if max_width == 0 {
            return WrappedMarkdown {
                lines: Vec::new(),
                scroll: 0,
            };
        }

        let mut lines = Vec::new();
        for (line, links) in self.text.lines.iter().zip(&self.line_links) {
            let line_style = self.text.style.patch(line.style);
            let graphemes = line.spans.iter().zip(links).flat_map(move |(span, link)| {
                let link_ref = link.as_ref();
                span.styled_graphemes(line_style)
                    .map(move |grapheme| LinkedGrapheme {
                        grapheme,
                        link: link_ref,
                    })
            });
            wrap_linked_line(graphemes, max_width, true, &mut lines);
        }

        WrappedMarkdown { lines, scroll: 0 }
    }
}

impl WrappedMarkdown<'_> {
    /// Returns the exact number of wrapped visual lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Sets the vertical scroll offset in visual lines.
    #[must_use]
    pub const fn scroll(mut self, offset: u16) -> Self {
        self.scroll = offset;
        self
    }
}

impl Widget for WrappedMarkdown<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Widget::render(&self, area, buf);
    }
}

impl Widget for &WrappedMarkdown<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }

        for (row, line) in self
            .lines
            .iter()
            .skip(self.scroll as usize)
            .take(area.height as usize)
            .enumerate()
        {
            render_wrapped_line(line, area, buf, area.top() + row as u16);
        }
    }
}

fn wrap_linked_line<'a>(
    line_symbols: impl IntoIterator<Item = LinkedGrapheme<'a>>,
    max_line_width: u16,
    trim: bool,
    output: &mut Vec<Vec<LinkedGrapheme<'a>>>,
) {
    let mut wrapped_lines: VecDeque<Vec<LinkedGrapheme<'a>>> = VecDeque::new();
    let mut pending_line: Vec<LinkedGrapheme<'a>> = Vec::new();
    let mut pending_word: Vec<LinkedGrapheme<'a>> = Vec::new();
    let mut pending_whitespace: VecDeque<LinkedGrapheme<'a>> = VecDeque::new();
    let mut line_width: u16 = 0;
    let mut word_width: u16 = 0;
    let mut whitespace_width: u16 = 0;
    let mut non_whitespace_previous = false;

    for item in line_symbols {
        let is_whitespace = item.grapheme.is_whitespace();
        let symbol_width = item.grapheme.symbol.cell_width();

        if symbol_width > max_line_width {
            continue;
        }

        let word_found = non_whitespace_previous && is_whitespace;
        let trimmed_overflow =
            pending_line.is_empty() && trim && word_width + symbol_width > max_line_width;
        let whitespace_overflow =
            pending_line.is_empty() && trim && whitespace_width + symbol_width > max_line_width;
        let untrimmed_overflow = pending_line.is_empty()
            && !trim
            && word_width + whitespace_width + symbol_width > max_line_width;

        if word_found || trimmed_overflow || whitespace_overflow || untrimmed_overflow {
            if !pending_line.is_empty() || !trim {
                pending_line.extend(pending_whitespace.drain(..));
                line_width += whitespace_width;
            }

            pending_line.append(&mut pending_word);
            line_width += word_width;

            pending_whitespace.clear();
            whitespace_width = 0;
            word_width = 0;
        }

        let line_full = line_width >= max_line_width;
        let pending_word_overflow = symbol_width > 0
            && line_width + whitespace_width + word_width + symbol_width > max_line_width;

        if line_full || pending_word_overflow {
            let mut remaining_width = max_line_width.saturating_sub(line_width);

            wrapped_lines.push_back(mem::take(&mut pending_line));
            line_width = 0;

            while let Some(ws) = pending_whitespace.front() {
                let width = ws.grapheme.symbol.cell_width();
                if width > remaining_width {
                    break;
                }
                whitespace_width -= width;
                remaining_width -= width;
                pending_whitespace.pop_front();
            }

            if is_whitespace && pending_whitespace.is_empty() {
                continue;
            }
        }

        if is_whitespace {
            whitespace_width += symbol_width;
            pending_whitespace.push_back(item);
        } else {
            word_width += symbol_width;
            pending_word.push(item);
        }

        non_whitespace_previous = !is_whitespace;
    }

    if pending_line.is_empty() && pending_word.is_empty() && !pending_whitespace.is_empty() && trim
    {
        wrapped_lines.push_back(Vec::new());
    }
    if !pending_line.is_empty() || !trim {
        pending_line.extend(pending_whitespace.drain(..));
    }
    pending_line.append(&mut pending_word);

    if !pending_line.is_empty() {
        wrapped_lines.push_back(pending_line);
    }
    if wrapped_lines.is_empty() {
        wrapped_lines.push_back(Vec::new());
    }

    output.extend(wrapped_lines);
}

fn render_wrapped_line(line: &[LinkedGrapheme<'_>], area: Rect, buf: &mut Buffer, y: u16) {
    let mut x = area.left();
    let mut i = 0;

    while i < line.len() && x < area.right() {
        if let Some(target) = line[i].link {
            // Collect all consecutive graphemes belonging to the same link target
            // and render them together with the Hyperlink widget.
            let mut spans: Vec<Span<'_>> = Vec::new();
            let mut current_style: Option<Style> = None;
            let mut current_text = String::new();
            let mut run_width: u16 = 0;

            while i < line.len() && line[i].link == Some(target) {
                let sym = line[i].grapheme.symbol;
                let width = sym.cell_width();
                if width == 0 {
                    i += 1;
                    continue;
                }
                if x.saturating_add(run_width).saturating_add(width) > area.right() {
                    break;
                }

                let style = line[i].grapheme.style;
                if current_style != Some(style) {
                    if !current_text.is_empty() {
                        spans.push(Span::styled(
                            mem::take(&mut current_text),
                            current_style.unwrap_or_default(),
                        ));
                    }
                    current_style = Some(style);
                }

                if sym.is_empty() {
                    current_text.push(' ');
                } else {
                    current_text.push_str(sym);
                }
                run_width = run_width.saturating_add(width);
                i += 1;
            }

            if !current_text.is_empty() {
                spans.push(Span::styled(
                    current_text,
                    current_style.unwrap_or_default(),
                ));
            }

            if run_width > 0 {
                let link_area = Rect::new(x, y, run_width, 1);
                Hyperlink::new(Text::from(Line::from(spans)), target.url.as_str())
                    .id(target.id.as_str())
                    .render(link_area, buf);
                x = x.saturating_add(run_width);
            } else {
                // Remaining link grapheme didn't fit on this row.
                break;
            }
        } else {
            let sym = line[i].grapheme.symbol;
            let width = sym.cell_width();
            if width > 0 {
                if x.saturating_add(width) > area.right() {
                    break;
                }
                let display = if sym.is_empty() { " " } else { sym };
                buf[(x, y)]
                    .set_symbol(display)
                    .set_style(line[i].grapheme.style);
                x = x.saturating_add(width);
            }
            i += 1;
        }
    }
}

fn make_link_id(seq: usize, url: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    let prime: u64 = 0x100000001b3;

    for byte in (seq as u64)
        .to_le_bytes()
        .into_iter()
        .chain(url.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(prime);
    }

    format!("{hash:016x}")
}

pub fn parse(input: &str, theme: Option<Theme>, max_width: u16) -> MarkdownDocument<'_> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    options.insert(Options::ENABLE_SUPERSCRIPT);
    options.insert(Options::ENABLE_SUBSCRIPT);
    let parser = Parser::new_ext(input, options);
    let mut writer = TextWriter::new(parser, theme, max_width);
    writer.run();
    MarkdownDocument {
        text: writer.text,
        line_links: writer.line_links,
    }
}

pub fn from_str(input: &str, theme: Option<Theme>, max_width: u16) -> Text<'_> {
    parse(input, theme, max_width).text
}

// Heading attributes collected from pulldown-cmark to render after the heading text.
struct HeadingMeta<'a> {
    id: Option<CowStr<'a>>,
    classes: Vec<CowStr<'a>>,
    attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
}

impl<'a> HeadingMeta<'a> {
    fn into_option(self) -> Option<Self> {
        let has_id = self.id.is_some();
        let has_classes = !self.classes.is_empty();
        let has_attrs = !self.attrs.is_empty();
        if has_id || has_classes || has_attrs {
            Some(self)
        } else {
            None
        }
    }

    // Format as a Markdown attribute block suffix, e.g. "{#id .class key=value}".
    fn to_suffix(&self) -> Option<String> {
        let mut parts = Vec::new();

        if let Some(id) = &self.id {
            parts.push(format!("#{}", id));
        }

        for class in &self.classes {
            parts.push(format!(".{}", class));
        }

        for (key, value) in &self.attrs {
            match value {
                Some(value) => parts.push(format!("{}={}", key, value)),
                None => parts.push(key.to_string()),
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(format!(" {{{}}}", parts.join(" ")))
        }
    }
}

struct TextWriter<'a, I> {
    /// Iterator supplying events.
    iter: I,

    /// Text to write to.
    text: Text<'a>,

    /// Current style.
    ///
    /// This is a stack of styles, with the top style being the current style.
    inline_styles: Vec<Style>,

    /// Prefix to add to the start of the each line.
    line_prefixes: Vec<Span<'a>>,

    /// Stack of line styles.
    line_styles: Vec<Style>,

    /// Used to highlight code blocks, set when  a codeblock is encountered
    code_highlighter: Option<HighlightLines<'a>>,

    /// The [`SyntaxSet`] that the current code_highlighter's syntax came from.
    code_syntax_set: Option<&'static SyntaxSet>,

    /// Current list index as a stack of indices.
    list_indices: Vec<Option<u64>>,

    /// A link which will be appended to the current line when the link tag is closed.
    link: Option<LinkTarget>,

    /// Monotonic counter used to assign distinct deterministic OSC 8 ids per link run.
    next_link_seq: usize,

    /// Parallel to `text.lines[i].spans[j]`: optional hyperlink target for each span.
    line_links: Vec<Vec<Option<LinkTarget>>>,

    /// The current image to be closed
    image: Option<CowStr<'a>>,

    /// Heading attributes to append after heading content.
    heading_meta: Option<HeadingMeta<'a>>,

    /// Whether we are inside a metadata block.
    in_metadata_block: bool,

    /// True when last element requires a new line
    needs_newline: bool,

    /// Buffer for highlighted code block lines before flushing.
    code_block_lines: Vec<Line<'a>>,

    /// The language identifier of the current code block.
    code_block_lang: String,

    /// Whether we are currently inside a code block (even without a highlighter).
    in_codeblock: bool,

    /// Maximum width of the rendered area (viewport width).
    max_width: u16,

    /// bulletty Theme
    theme: Option<Theme>,
}

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static EXTRA_SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(|| {
    let mut builder = SyntaxSetBuilder::new();
    builder.add_plain_text_syntax();
    const FILES: &[&str] = &[
        include_str!("../../../res/syntaxes/TOML.sublime-syntax"),
        include_str!("../../../res/syntaxes/Dockerfile.sublime-syntax"),
        include_str!("../../../res/syntaxes/CMake.sublime-syntax"),
        include_str!("../../../res/syntaxes/INI.sublime-syntax"),
        include_str!("../../../res/syntaxes/DotENV.sublime-syntax"),
        include_str!("../../../res/syntaxes/GraphQL.sublime-syntax"),
        include_str!("../../../res/syntaxes/nginx.sublime-syntax"),
    ];
    for src in FILES {
        if let Ok(def) = SyntaxDefinition::load_from_str(src, true, None) {
            builder.add(def);
        }
    }
    builder.build()
});
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

impl<'a, I> TextWriter<'a, I>
where
    I: Iterator<Item = Event<'a>>,
{
    fn new(iter: I, theme: Option<Theme>, max_width: u16) -> Self {
        Self {
            iter,
            text: Text::default(),
            inline_styles: vec![],
            line_styles: vec![],
            line_prefixes: vec![],
            list_indices: vec![],
            needs_newline: false,
            code_highlighter: None,
            code_syntax_set: None,
            link: None,
            next_link_seq: 0,
            line_links: vec![],
            image: None,
            heading_meta: None,
            in_metadata_block: false,
            code_block_lines: vec![],
            code_block_lang: String::new(),
            in_codeblock: false,
            max_width,
            theme,
        }
    }

    fn run(&mut self) {
        debug!("Running text writer");
        while let Some(event) = self.iter.next() {
            self.handle_event(event);
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn handle_event(&mut self, event: Event<'a>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(text),
            Event::Code(code) => self.code(code),
            Event::Html(_html) => warn!("Html not yet supported"),
            Event::InlineHtml(_html) => warn!("Inline html not yet supported"),
            Event::FootnoteReference(_) => warn!("Footnote reference not yet supported"),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.rule(),
            Event::TaskListMarker(checked) => self.task_list_marker(checked),
            Event::InlineMath(_) => warn!("Inline math not yet supported"),
            Event::DisplayMath(_) => warn!("Display math not yet supported"),
        }
    }

    fn start_tag(&mut self, tag: Tag<'a>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => self.start_heading(level, HeadingMeta { id, classes, attrs }),
            Tag::BlockQuote(kind) => self.start_blockquote(kind),
            Tag::CodeBlock(kind) => self.start_codeblock(kind),
            Tag::HtmlBlock => warn!("Html block not yet supported"),
            Tag::List(start_index) => self.start_list(start_index),
            Tag::Item => self.start_item(),
            Tag::FootnoteDefinition(_) => warn!("Footnote definition not yet supported"),
            Tag::Table(_) => warn!("Table not yet supported"),
            Tag::TableHead => warn!("Table head not yet supported"),
            Tag::TableRow => warn!("Table row not yet supported"),
            Tag::TableCell => warn!("Table cell not yet supported"),
            Tag::Emphasis => self.push_inline_style(Style::new().italic()),
            Tag::Strong => self.push_inline_style(Style::new().bold()),
            Tag::Strikethrough => self.push_inline_style(Style::new().crossed_out()),
            Tag::Subscript => self.push_inline_style(Style::new().dim().italic()),
            Tag::Superscript => self.push_inline_style(Style::new().dim().italic()),
            Tag::Link { dest_url, .. } => self.push_link(dest_url),
            Tag::Image {
                link_type,
                dest_url,
                title,
                ..
            } => self.push_image(link_type, dest_url, title),
            Tag::MetadataBlock(_) => self.start_metadata_block(),
            Tag::DefinitionList => warn!("Definition list not yet supported"),
            Tag::DefinitionListTitle => warn!("Definition list title not yet supported"),
            Tag::DefinitionListDefinition => warn!("Definition list definition not yet supported"),
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.end_paragraph(),
            TagEnd::Heading(_) => self.end_heading(),
            TagEnd::BlockQuote(_) => self.end_blockquote(),
            TagEnd::CodeBlock => self.end_codeblock(),
            TagEnd::HtmlBlock => {}
            TagEnd::List(_is_ordered) => self.end_list(),
            TagEnd::Item => {}
            TagEnd::FootnoteDefinition => {}
            TagEnd::Table => {}
            TagEnd::TableHead => {}
            TagEnd::TableRow => {}
            TagEnd::TableCell => {}
            TagEnd::Emphasis => self.pop_inline_style(),
            TagEnd::Strong => self.pop_inline_style(),
            TagEnd::Strikethrough => self.pop_inline_style(),
            TagEnd::Subscript => self.pop_inline_style(),
            TagEnd::Superscript => self.pop_inline_style(),
            TagEnd::Link => self.pop_link(),
            TagEnd::Image => self.pop_image(),
            TagEnd::MetadataBlock(_) => self.end_metadata_block(),
            TagEnd::DefinitionList => {}
            TagEnd::DefinitionListTitle => {}
            TagEnd::DefinitionListDefinition => {}
        }
    }

    fn start_paragraph(&mut self) {
        // Insert an empty line between paragraphs if there is at least one line of text already.
        if self.needs_newline {
            self.push_line(Line::default());
        }

        self.push_line(Line::default().style(styles::p(self.theme.as_ref())));
        self.needs_newline = false;
    }

    fn end_paragraph(&mut self) {
        self.needs_newline = true
    }

    fn start_heading(&mut self, level: HeadingLevel, heading_meta: HeadingMeta<'a>) {
        if self.needs_newline {
            self.push_line(Line::default());
        }

        let style = match level {
            HeadingLevel::H1 => styles::h1(self.theme.as_ref()),
            HeadingLevel::H2 => styles::h2(self.theme.as_ref()),
            HeadingLevel::H3 => styles::h3(self.theme.as_ref()),
            HeadingLevel::H4 => styles::h4(self.theme.as_ref()),
            HeadingLevel::H5 => styles::h5(self.theme.as_ref()),
            HeadingLevel::H6 => styles::h6(self.theme.as_ref()),
        };

        let content = format!("{} ", "#".repeat(level as usize));
        self.push_line(Line::styled(content, style));
        self.heading_meta = heading_meta.into_option();
        self.needs_newline = false;
    }

    fn end_heading(&mut self) {
        if let Some(meta) = self.heading_meta.take() {
            if let Some(suffix) = meta.to_suffix() {
                self.push_span(Span::styled(
                    suffix,
                    styles::heading_meta(self.theme.as_ref()),
                ));
            }
        }
        self.needs_newline = true
    }

    fn start_blockquote(&mut self, _kind: Option<BlockQuoteKind>) {
        if self.needs_newline {
            self.push_line(Line::default());
            self.needs_newline = false;
        }
        self.line_prefixes.push(Span::from(">"));
        self.line_styles
            .push(styles::blockquote(self.theme.as_ref()));
    }

    fn end_blockquote(&mut self) {
        self.line_prefixes.pop();
        self.line_styles.pop();
        self.needs_newline = true;
    }

    fn text(&mut self, text: CowStr<'a>) {
        if let Some(highlighter) = &mut self.code_highlighter {
            let set = self.code_syntax_set.unwrap();
            let expanded = text.replace('\t', "  ");
            let text: Text = LinesWithEndings::from(&expanded)
                .filter_map(|line| highlighter.highlight_line(line, set).ok())
                .filter_map(|part| as_24_bit_terminal_escaped(&part, false).into_text().ok())
                .flatten()
                .collect();

            for line in text.lines {
                self.code_block_lines.push(line);
            }
            self.needs_newline = false;
            return;
        }

        if self.in_codeblock {
            let code_style = styles::code(self.theme.as_ref());
            for line in text.lines() {
                self.code_block_lines
                    .push(Line::styled(line.replace('\t', "  "), code_style));
            }
            self.needs_newline = false;
            return;
        }

        for (position, line) in text.lines().with_position() {
            if self.needs_newline {
                self.push_line(Line::default());
                self.needs_newline = false;
            }
            if !position.is_first() {
                self.push_line(Line::default());
            }

            let style = self.inline_styles.last().copied().unwrap_or_default();

            let span = Span::styled(line.to_owned(), style);

            self.push_span(span);
        }
        self.needs_newline = false;
    }

    fn code(&mut self, code: CowStr<'a>) {
        let span = Span::styled(code, styles::code(self.theme.as_ref()));
        self.push_span(span);
    }

    fn hard_break(&mut self) {
        self.push_line(Line::default());
    }

    fn start_metadata_block(&mut self) {
        if self.needs_newline {
            self.push_line(Line::default());
        }
        self.line_styles.push(styles::metadata(self.theme.as_ref()));
        self.push_line(Line::from("---"));
        self.push_line(Line::default());
        self.in_metadata_block = true;
    }

    fn end_metadata_block(&mut self) {
        if self.in_metadata_block {
            self.push_line(Line::from("---"));
            self.line_styles.pop();
            self.in_metadata_block = false;
            self.needs_newline = true;
        }
    }

    fn rule(&mut self) {
        if self.needs_newline {
            self.push_line(Line::default());
        }
        self.push_line(Line::from("---"));
        self.needs_newline = true;
    }

    fn start_list(&mut self, index: Option<u64>) {
        if self.list_indices.is_empty() && self.needs_newline {
            self.push_line(Line::default());
        }
        self.list_indices.push(index);
        self.inline_styles
            .push(styles::list_item(self.theme.as_ref()));
    }

    fn end_list(&mut self) {
        self.list_indices.pop();
        self.inline_styles.pop();
        self.needs_newline = true;
    }

    fn start_item(&mut self) {
        self.push_line(Line::default());
        let width = self.list_indices.len() * 4 - 3;
        if let Some(last_index) = self.list_indices.last_mut() {
            let span = match last_index {
                None => Span::from(" ".repeat(width - 1) + "\u{00A0}\u{00A0}• "),
                Some(index) => {
                    *index += 1;
                    Span::from(format!("\u{00A0}\u{00A0}{:width$}. ", *index - 1))
                }
            };
            self.push_span(span.style(styles::list_item(self.theme.as_ref())));
        }
        self.needs_newline = false;
    }

    fn task_list_marker(&mut self, checked: bool) {
        let marker = if checked { 'x' } else { ' ' };
        let marker_span = Span::from(format!("[{}] ", marker));
        if let Some(line) = self.text.lines.last_mut() {
            if let Some(first_span) = line.spans.first_mut() {
                let content = first_span.content.to_mut();
                if content.ends_with("• ") {
                    let len = content.len();
                    content.truncate(len - 4); // "• " is 4 bytes
                    content.push_str("• [");
                    content.push(marker);
                    content.push_str("] ");
                    return;
                }

                // Check for numbered list format
                if content.ends_with(". ") {
                    let len = content.len();
                    content.truncate(len - 1);
                    content.push_str(" [");
                    content.push(marker);
                    content.push_str("] ");
                    return;
                }
            }
            line.spans.insert(1, marker_span);
            if let Some(links) = self.line_links.last_mut() {
                links.insert(1, None);
            }
        } else {
            self.push_span(marker_span);
        }
    }

    fn soft_break(&mut self) {
        if self.in_metadata_block {
            self.hard_break();
        } else {
            self.push_line(Line::default().style(styles::p(self.theme.as_ref())));
        }
    }

    fn start_codeblock(&mut self, kind: CodeBlockKind<'_>) {
        if !self.text.lines.is_empty() {
            self.push_line(Line::default());
        }
        let lang = match kind {
            CodeBlockKind::Fenced(ref lang) => lang.as_ref(),
            CodeBlockKind::Indented => "",
        };

        self.line_styles.push(styles::code(self.theme.as_ref()));

        self.set_code_highlighter(lang);

        self.code_block_lines.clear();
        self.code_block_lang.clear();
        self.code_block_lang.push_str(lang);
        self.in_codeblock = true;
    }

    fn end_codeblock(&mut self) {
        self.in_codeblock = false;
        self.flush_codeblock();

        self.line_styles.pop();

        self.clear_code_highlighter();
    }

    fn flush_codeblock(&mut self) {
        let code_style = styles::code(self.theme.as_ref());
        let bg = code_style.bg.unwrap_or(Color::Black);

        let lines = std::mem::take(&mut self.code_block_lines);
        let lang = std::mem::take(&mut self.code_block_lang);

        let box_width = self.max_width as usize;
        if box_width < 4 {
            return;
        }

        let has_lang = !lang.is_empty();
        let lang_width = UnicodeWidthStr::width(lang.as_str());

        let inner_width = box_width.saturating_sub(4);

        if has_lang {
            let lang_space = lang_width + 2;
            if inner_width >= lang_space {
                let dashes = inner_width - lang_space;
                let left = dashes / 2;
                let right = dashes - left;
                let top = format!("┌{} {} {}┐", "─".repeat(left), lang, "─".repeat(right + 2));
                self.push_line(Line::styled(top, code_style));
            } else {
                let top = format!("┌{}┐", "─".repeat(box_width.saturating_sub(2)));
                self.push_line(Line::styled(top, code_style));
            }
        } else {
            let top = format!("┌{}┐", "─".repeat(box_width.saturating_sub(2)));
            self.push_line(Line::styled(top, code_style));
        }

        for line in lines {
            let wrapped = wrap_code_line(line, inner_width, bg, code_style);
            for wline in wrapped {
                let span_count = wline.spans.len();
                self.text.lines.push(wline);
                self.line_links.push(vec![None; span_count]);
            }
        }

        let bottom = format!("└{}┘", "─".repeat(box_width.saturating_sub(2)));
        self.push_line(Line::styled(bottom, code_style));

        self.needs_newline = true;
    }

    #[instrument(level = "trace", skip(self))]
    fn set_code_highlighter(&mut self, lang: &str) {
        let resolved = match lang {
            "shell" => "sh",
            "bash" => "sh",
            "yaml" => "yml",
            "docker" => "Dockerfile",
            "dockerfile" => "Dockerfile",
            "Containerfile" => "Dockerfile",
            "Caddyfile" => "nginx",
            other => other,
        };
        let maybe_pair = SYNTAX_SET
            .find_syntax_by_token(resolved)
            .map(|s| (s, &*SYNTAX_SET))
            .or_else(|| {
                EXTRA_SYNTAX_SET
                    .find_syntax_by_token(resolved)
                    .map(|s| (s, &*EXTRA_SYNTAX_SET))
            });
        match maybe_pair {
            Some((syntax, set)) => {
                debug!(
                    "Starting code block with syntax: {:?} (resolved: {:?})",
                    lang, resolved
                );
                let theme = &THEME_SET.themes["base16-ocean.dark"];
                let highlighter = HighlightLines::new(syntax, theme);
                self.code_highlighter = Some(highlighter);
                self.code_syntax_set = Some(set);
            }
            None => {
                warn!("Could not find syntax for code block: {:?}", lang);
            }
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn clear_code_highlighter(&mut self) {
        self.code_highlighter = None;
        self.code_syntax_set = None;
    }

    #[instrument(level = "trace", skip(self))]
    fn push_inline_style(&mut self, style: Style) {
        let current_style = self.inline_styles.last().copied().unwrap_or_default();
        let style = current_style.patch(style);
        self.inline_styles.push(style);
        debug!("Pushed inline style: {:?}", style);
        debug!("Current inline styles: {:?}", self.inline_styles);
    }

    #[instrument(level = "trace", skip(self))]
    fn pop_inline_style(&mut self) {
        self.inline_styles.pop();
    }

    #[instrument(level = "trace", skip(self))]
    fn push_line(&mut self, line: Line<'a>) {
        let style = self.line_styles.last().copied().unwrap_or_default();
        let mut line = line.patch_style(style);

        // Add line prefixes to the start of the line.
        let line_prefixes = self.line_prefixes.iter().cloned().collect_vec();
        let has_prefixes = !line_prefixes.is_empty();
        if has_prefixes {
            line.spans.insert(0, " ".into());
        }
        for prefix in line_prefixes.iter().rev().cloned() {
            line.spans.insert(0, prefix);
        }
        let span_count = line.spans.len();
        self.text.lines.push(line);
        self.line_links.push(vec![None; span_count]);
    }

    #[instrument(level = "trace", skip(self))]
    fn push_span(&mut self, span: Span<'a>) {
        let link = self.link.clone();
        self.push_span_with_link(span, link);
    }

    fn push_span_with_link(&mut self, span: Span<'a>, link: Option<LinkTarget>) {
        if let Some(line) = self.text.lines.last_mut() {
            line.push_span(span);
            self.line_links.last_mut().unwrap().push(link);
        } else {
            self.push_line(Line::from(vec![span]));
            if let Some(links) = self.line_links.last_mut()
                && let Some(last) = links.last_mut()
            {
                *last = link;
            }
        }
    }

    /// Store the link to be appended to the link text
    #[instrument(level = "trace", skip(self))]
    fn push_link(&mut self, dest_url: CowStr<'a>) {
        let url = dest_url.into_string();
        let id = make_link_id(self.next_link_seq, &url);
        self.next_link_seq += 1;
        self.link = Some(LinkTarget { url, id });
        self.push_inline_style(styles::link(self.theme.as_ref()));
    }

    /// Append the link to the current line
    #[instrument(level = "trace", skip(self))]
    fn pop_link(&mut self) {
        if let Some(link) = self.link.take() {
            self.pop_inline_style();
            let url = link.url.clone();
            self.push_span_with_link(" (".into(), None);
            self.push_span_with_link(
                Span::styled(url, styles::link(self.theme.as_ref())),
                Some(link),
            );
            self.push_span_with_link(")".into(), None);
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn push_image(
        &mut self,
        _link_type: pulldown_cmark::LinkType,
        dest_url: CowStr<'a>,
        _title: CowStr<'a>,
    ) {
        self.image = Some(dest_url);
        let text = "[Image: ";
        self.push_line(Line::styled(text, styles::p(self.theme.as_ref())));
    }

    /// Append the end of the image tag
    #[instrument(level = "trace", skip(self))]
    fn pop_image(&mut self) {
        if let Some(image_link) = self.image.take() {
            let url = image_link.to_string();
            let url_target = LinkTarget {
                id: make_link_id(self.next_link_seq, &url),
                url,
            };
            self.next_link_seq += 1;
            self.push_span_with_link("  -> ".into(), None);
            self.push_span_with_link(
                Span::styled(image_link, styles::link(self.theme.as_ref())),
                Some(url_target),
            );
            self.push_span_with_link("]".into(), None);
        }
    }
}

fn split_str_at_width(s: &str, max_width: usize) -> (&str, &str) {
    let mut width = 0;
    for (i, c) in s.char_indices() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + cw > max_width {
            return (&s[..i], &s[i..]);
        }
        width += cw;
    }
    (s, "")
}

fn fill_line(line: Line<'_>, inner_width: usize, bg: Color, code_style: Style) -> Line<'_> {
    let line_width = line.width();
    let padding = inner_width.saturating_sub(line_width);
    let pad_bg = Style::new().bg(bg);
    let mut spans: Vec<Span> = vec![Span::styled("│ ", code_style)];
    for span in line.spans {
        spans.push(Span::styled(span.content, span.style.bg(bg)));
    }
    if padding > 0 {
        spans.push(Span::styled(" ".repeat(padding), pad_bg));
    }
    spans.push(Span::styled(" │", code_style));
    Line::from(spans)
}

fn wrap_code_line<'a>(
    mut line: Line<'a>,
    inner_width: usize,
    bg: Color,
    code_style: Style,
) -> Vec<Line<'a>> {
    let line_width = line.width();
    if inner_width == 0 {
        return vec![fill_line(line, 0, bg, code_style)];
    }
    if line_width <= inner_width {
        return vec![fill_line(line, inner_width, bg, code_style)];
    }

    let mut spans = std::mem::take(&mut line.spans);
    let mut result = Vec::new();
    let mut span_idx = 0;

    while span_idx < spans.len() {
        let mut current_spans: Vec<Span> = Vec::new();
        let mut current_width = 0;

        loop {
            if span_idx >= spans.len() {
                break;
            }
            let span = &spans[span_idx];
            let style = span.style.bg(bg);
            let span_width = span.width();
            let remaining = inner_width.saturating_sub(current_width);

            if span_width <= remaining {
                current_spans.push(Span::styled(span.content.to_string(), style.clone()));
                current_width += span_width;
                span_idx += 1;
            } else if remaining == 0 {
                break;
            } else {
                let content = span.content.as_ref();
                let (first, second) = split_str_at_width(content, remaining);
                if !first.is_empty() {
                    current_spans.push(Span::styled(first.to_string(), style.clone()));
                }
                spans[span_idx] = Span::styled(second.to_string(), style);
                break;
            }
        }

        result.push(fill_line(
            Line::from(current_spans),
            inner_width,
            bg,
            code_style,
        ));
    }

    result
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use ratatui::style::{Color, Stylize};
    use rstest::{fixture, rstest};
    use tracing::level_filters::LevelFilter;
    use tracing::subscriber::{self, DefaultGuard};
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::fmt::time::Uptime;

    use super::*;

    #[fixture]
    fn with_tracing() -> DefaultGuard {
        let subscriber = tracing_subscriber::fmt()
            .with_test_writer()
            .with_timer(Uptime::default())
            .with_max_level(LevelFilter::TRACE)
            .with_span_events(FmtSpan::ENTER)
            .finish();
        subscriber::set_default(subscriber)
    }

    #[rstest]
    fn empty(_with_tracing: DefaultGuard) {
        assert_eq!(from_str("", None, 80), Text::default());
    }

    #[rstest]
    fn paragraph_single(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("Hello, world!", None, 80),
            Text::from(Line::from("Hello, world!").style(styles::p(None)))
        );
    }

    #[rstest]
    fn paragraph_soft_break(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                Hello
                World
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from("Hello").style(styles::p(None)),
                Line::from("World").style(styles::p(None)),
            ])
        );
    }

    #[rstest]
    fn paragraph_multiple(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                Paragraph 1

                Paragraph 2
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from("Paragraph 1").style(styles::p(None)),
                Line::default(),
                Line::from("Paragraph 2").style(styles::p(None))
            ])
        );
    }

    #[rstest]
    fn rule(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                Paragraph 1

                ---

                Paragraph 2
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from("Paragraph 1").style(styles::p(None)),
                Line::default(),
                Line::from("---"),
                Line::default(),
                Line::from("Paragraph 2").style(styles::p(None))
            ])
        );
    }

    #[rstest]
    fn headings(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                # Heading 1
                ## Heading 2
                ### Heading 3
                #### Heading 4
                ##### Heading 5
                ###### Heading 6
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter(["# ", "Heading 1"]).style(styles::h1(None)),
                Line::default(),
                Line::from_iter(["## ", "Heading 2"]).style(styles::h2(None)),
                Line::default(),
                Line::from_iter(["### ", "Heading 3"]).style(styles::h3(None)),
                Line::default(),
                Line::from_iter(["#### ", "Heading 4"]).style(styles::h4(None)),
                Line::default(),
                Line::from_iter(["##### ", "Heading 5"]).style(styles::h5(None)),
                Line::default(),
                Line::from_iter(["###### ", "Heading 6"]).style(styles::h6(None)),
            ])
        );
    }

    #[rstest]
    fn heading_attributes(_with_tracing: DefaultGuard) {
        let h1 = styles::h1(None);
        let meta = styles::heading_meta(None);

        assert_eq!(
            from_str("# Heading {#title .primary data-kind=doc}", None, 80),
            Text::from(
                Line::from_iter([
                    Span::from("# "),
                    Span::from("Heading"),
                    Span::styled(" {#title .primary data-kind=doc}", meta),
                ])
                .style(h1)
            )
        );
    }

    /// I was having difficulty getting the right number of newlines between paragraphs, so this
    /// test is to help debug and ensure that.
    #[rstest]
    fn blockquote_after_paragraph(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                Hello, world!

                > Blockquote
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from("Hello, world!").style(styles::blockquote(None)),
                Line::default(),
                Line::from_iter([">", " ", "Blockquote"]).style(styles::blockquote(None)),
            ])
        );
    }
    #[rstest]
    fn blockquote_single(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("> Blockquote", None, 80),
            Text::from(Line::from_iter([">", " ", "Blockquote"]).style(styles::blockquote(None)))
        );
    }

    #[rstest]
    fn blockquote_soft_break(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                > Blockquote 1
                > Blockquote 2
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::blockquote(None)),
                Line::from_iter([">", " ", "Blockquote 2"]).style(styles::blockquote(None)),
            ])
        );
    }

    #[rstest]
    fn blockquote_multiple(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                > Blockquote 1
                >
                > Blockquote 2
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::blockquote(None)),
                Line::from_iter([">", " "]).style(styles::blockquote(None)),
                Line::from_iter([">", " ", "Blockquote 2"]).style(styles::blockquote(None)),
            ])
        );
    }

    #[rstest]
    fn blockquote_multiple_with_break(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                > Blockquote 1

                > Blockquote 2
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::blockquote(None)),
                Line::default(),
                Line::from_iter([">", " ", "Blockquote 2"]).style(styles::blockquote(None)),
            ])
        );
    }

    #[rstest]
    fn blockquote_nested(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                > Blockquote 1
                >> Nested Blockquote
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::blockquote(None)),
                Line::from_iter([">", " "]).style(styles::blockquote(None)),
                Line::from_iter([">", ">", " ", "Nested Blockquote"])
                    .style(styles::blockquote(None)),
            ])
        );
    }

    #[rstest]
    fn list_single(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                - List item 1
            "},
                None,
                80
            ),
            Text::from(Line::from_iter([
                Span::from("\u{a0}\u{a0}• ").style(styles::list_item(None)),
                Span::from("List item 1").style(styles::list_item(None))
            ]))
        );
    }

    #[rstest]
    fn list_multiple(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                - List item 1
                - List item 2
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter([
                    Span::from("\u{a0}\u{a0}• ").style(styles::list_item(None)),
                    Span::from("List item 1").style(styles::list_item(None))
                ]),
                Line::from_iter([
                    Span::from("\u{a0}\u{a0}• ").style(styles::list_item(None)),
                    Span::from("List item 2").style(styles::list_item(None))
                ]),
            ])
        );
    }

    #[rstest]
    fn list_ordered(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                1. List item 1
                2. List item 2
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter([
                    Span::from("\u{a0}\u{a0}1. ").style(styles::list_item(None)),
                    Span::from("List item 1").style(styles::list_item(None))
                ]),
                Line::from_iter([
                    Span::from("\u{a0}\u{a0}2. ").style(styles::list_item(None)),
                    Span::from("List item 2").style(styles::list_item(None))
                ]),
            ])
        );
    }

    #[rstest]
    fn list_nested(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                - List item 1
                  - Nested list item 1
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from_iter([
                    Span::from("\u{a0}\u{a0}• ").style(styles::list_item(None)),
                    Span::from("List item 1").style(styles::list_item(None))
                ]),
                Line::from_iter([
                    Span::from("    \u{a0}\u{a0}• ").style(styles::list_item(None)),
                    Span::from("Nested list item 1").style(styles::list_item(None))
                ]),
            ])
        );
    }

    #[rstest]
    fn list_task_items(_with_tracing: DefaultGuard) {
        let result = from_str(
            indoc! {"
                - [ ] Incomplete
                - [x] Complete
            "},
            None,
            80,
        );
        // Just verify it parses without error and has the right number of lines
        assert_eq!(result.lines.len(), 2);
    }

    #[rstest]
    fn strong(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("**Strong**", None, 80),
            Text::from(Line::from("Strong".bold()).style(styles::p(None)))
        );
    }

    #[rstest]
    fn emphasis(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("*Emphasis*", None, 80),
            Text::from(Line::from("Emphasis".italic()).style(styles::p(None)))
        );
    }

    #[rstest]
    fn strikethrough(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("~~Strikethrough~~", None, 80),
            Text::from(Line::from("Strikethrough".crossed_out()).style(styles::p(None)))
        );
    }

    #[rstest]
    fn strong_emphasis(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("**Strong *emphasis***", None, 80),
            Text::from(
                Line::from_iter(["Strong ".bold(), "emphasis".bold().italic()])
                    .style(styles::p(None))
            )
        );
    }

    #[rstest]
    fn superscript(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("H ^2^ O", None, 80),
            Text::from(
                Line::from_iter([
                    Span::from("H "),
                    Span::styled("2", Style::new().dim().italic()),
                    Span::from(" O"),
                ])
                .fg(Color::Rgb(255, 255, 255))
            )
        );
    }

    #[rstest]
    fn subscript(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("H ~2~ O", None, 80),
            Text::from(
                Line::from_iter([
                    Span::from("H "),
                    Span::styled("2", Style::new().dim().italic()),
                    Span::from(" O"),
                ])
                .fg(Color::Rgb(255, 255, 255))
            )
        );
    }

    #[rstest]
    fn metadata_block(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(
                indoc! {"
                ---
                title: Demo
                ---

                Body
            "},
                None,
                80
            ),
            Text::from_iter([
                Line::from("---").style(Style::new().fg(Color::Rgb(255, 255, 255))),
                Line::from("title: Demo").style(Style::new().fg(Color::Rgb(255, 255, 255))),
                Line::from("---").style(Style::new().fg(Color::Rgb(255, 255, 255))),
                Line::default(),
                Line::from("Body").fg(Color::Rgb(255, 255, 255)),
            ])
        );
    }

    #[rstest]
    fn link(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("[Link](https://example.com)", None, 80),
            Text::from(
                Line::from_iter([
                    Span::styled("Link", styles::link(None)),
                    Span::from(" ("),
                    Span::styled("https://example.com", styles::link(None)),
                    Span::from(")")
                ])
                .style(styles::p(None))
            )
        );
    }

    #[rstest]
    fn image(_with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("![TestImage](/test.html)", None, 80),
            Text::from_iter([
                Line::default().style(styles::p(None)),
                Line::from_iter([
                    Span::from("[Image: "),
                    Span::from("TestImage"),
                    Span::from("  -> "),
                    Span::styled("/test.html", styles::link(None)),
                    Span::from("]"),
                ])
                .style(styles::p(None)),
            ])
        );
    }

    #[rstest]
    fn renders_markdown_link_text_and_url_as_osc8(_with_tracing: DefaultGuard) {
        let url = "https://example.com";
        let md = format!("See [Click **me**]({url}) now");
        let doc = parse(&md, None, 80);
        let wrapped = doc.wrap(80);
        let area = Rect::new(0, 0, 80, 2);
        let mut buf = Buffer::empty(area);
        wrapped.render(area, &mut buf);

        // "See " (cols 0..4) should be plain text.
        for (x, ch) in "See ".chars().enumerate() {
            assert_eq!(buf[(x as u16, 0)].symbol(), ch.to_string());
        }

        // "Click me" (cols 4..12) should be OSC 8 linked with one shared id,
        // styled with link style and preserving BOLD on "me".
        let link_id = buf[(4, 0)]
            .symbol()
            .strip_prefix("\x1b]8;id=")
            .and_then(|s| s.split(';').next())
            .expect("link text should have OSC 8 id")
            .to_string();
        let expected_link_fg = styles::link(None).fg.unwrap();
        for (i, ch) in "Click me".chars().enumerate() {
            let cell = &buf[(4 + i as u16, 0)];
            assert_eq!(
                cell.symbol(),
                format!("\x1b]8;id={link_id};{url}\x1b\\{ch}\x1b]8;;\x1b\\")
            );
            assert_eq!(cell.fg, expected_link_fg);
            assert!(!cell.modifier.contains(ratatui::style::Modifier::UNDERLINED));
        }
        assert!(
            buf[(10, 0)]
                .modifier
                .contains(ratatui::style::Modifier::BOLD)
        );

        // " (" (cols 12..14) is plain punctuation.
        assert_eq!(buf[(12, 0)].symbol(), " ");
        assert_eq!(buf[(13, 0)].symbol(), "(");

        // URL (cols 14..14+url.len()) should share the same OSC 8 id and link style.
        for (i, ch) in url.chars().enumerate() {
            let cell = &buf[(14 + i as u16, 0)];
            assert_eq!(
                cell.symbol(),
                format!("\x1b]8;id={link_id};{url}\x1b\\{ch}\x1b]8;;\x1b\\")
            );
            assert_eq!(cell.fg, expected_link_fg);
            assert!(!cell.modifier.contains(ratatui::style::Modifier::UNDERLINED));
        }

        // ") now" after URL is plain text.
        let after = 14 + url.len() as u16;
        assert_eq!(buf[(after, 0)].symbol(), ")");
    }

    #[rstest]
    fn renders_wrapped_markdown_link_with_shared_osc8_id(_with_tracing: DefaultGuard) {
        let url = "https://example.com/very/long/path";
        let md = format!("[Link]({url})");
        let doc = parse(&md, None, 20);
        let wrapped = doc.wrap(20);
        assert!(wrapped.line_count() >= 3);

        let area = Rect::new(0, 0, 20, 4);
        let mut buf = Buffer::empty(area);
        wrapped.render(area, &mut buf);

        let url_part = format!(";{url}\x1b\\");
        // Line 0 has "Link", line 1 has "(https://example.com" (URL starts at x = 1),
        // line 2 has "/very/long/path)" (starts at x = 0). All three share one OSC 8 id.
        let id_row0 = buf[(0, 0)]
            .symbol()
            .strip_prefix("\x1b]8;id=")
            .and_then(|s| s.split(';').next())
            .expect("link title line should have OSC 8 id");
        let id_row1 = buf[(1, 1)]
            .symbol()
            .strip_prefix("\x1b]8;id=")
            .and_then(|s| s.split(';').next())
            .expect("first wrapped URL line should have OSC 8 id");
        let id_row2 = buf[(0, 2)]
            .symbol()
            .strip_prefix("\x1b]8;id=")
            .and_then(|s| s.split(';').next())
            .expect("second wrapped URL line should have OSC 8 id");

        assert!(buf[(0, 0)].symbol().contains(&url_part));
        assert!(buf[(1, 1)].symbol().contains(&url_part));
        assert!(buf[(0, 2)].symbol().contains(&url_part));
        assert_eq!(id_row0, id_row1);
        assert_eq!(id_row1, id_row2);
    }

    #[rstest]
    fn renders_markdown_image_url_as_osc8(_with_tracing: DefaultGuard) {
        let img_url = "https://example.com/pic.png";
        let md = format!("![Alt]({img_url})");
        let doc = parse(&md, None, 80);
        let wrapped = doc.wrap(80);
        let area = Rect::new(0, 0, 80, 3);
        let mut buf = Buffer::empty(area);
        wrapped.render(area, &mut buf);

        let expected = format!(";{img_url}\x1b\\");
        let linked_cells = buf
            .content()
            .iter()
            .filter(|c| c.symbol().starts_with("\x1b]8;id=") && c.symbol().contains(&expected))
            .count();
        assert_eq!(linked_cells, img_url.len());
    }
}
