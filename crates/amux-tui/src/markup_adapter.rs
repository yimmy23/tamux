//! Custom adapter that uses ftui markup parsing instead of Text::raw().
//!
//! The standard `StringModelAdapter` calls `Text::raw()` which treats
//! everything as plain text — ANSI escapes or markup tags appear as
//! literal characters. This adapter runs the view string through
//! `parse_markup()` so that `[fg=...]`, `[bg=...]`, `[bold]`, etc.
//! are interpreted as styled text.

use ftui_core::event::Event;
use ftui_render::cell::{Cell, CellContent};
use ftui_render::frame::Frame;
use ftui_runtime::program::{Cmd, Model};
use ftui_runtime::string_model::StringModel;
use ftui_text::markup::parse_markup;
use ftui_text::{Text, grapheme_width};
use unicode_segmentation::UnicodeSegmentation;

/// Adapter that bridges a [`StringModel`] to the full [`Model`] trait,
/// parsing the view string as ftui markup instead of raw text.
pub struct MarkupModelAdapter<S: StringModel> {
    inner: S,
}

impl<S: StringModel> MarkupModelAdapter<S> {
    /// Create a new adapter wrapping the given string model.
    #[inline]
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: StringModel> Model for MarkupModelAdapter<S>
where
    S::Message: From<Event> + Send + 'static,
{
    type Message = S::Message;

    fn init(&mut self) -> Cmd<Self::Message> {
        self.inner.init()
    }

    fn update(&mut self, msg: Self::Message) -> Cmd<Self::Message> {
        self.inner.update(msg)
    }

    fn view(&self, frame: &mut Frame) {
        let s = self.inner.view_string();
        let fixed = fix_nested_tags(&s);
        // parse_markup() doesn't split on \n — it produces a single Line.
        // We must split the view string into lines first, parse each one
        // separately, then combine into a multi-line Text.
        let mut all_lines = Vec::new();
        for line_str in fixed.split('\n') {
            match parse_markup(line_str) {
                Ok(parsed) => {
                    // Take the first (and only) line from the parsed result
                    if let Some(line) = parsed.lines().first() {
                        all_lines.push(line.clone());
                    } else {
                        all_lines.push(ftui_text::Line::raw(""));
                    }
                }
                Err(_) => {
                    all_lines.push(ftui_text::Line::raw(line_str));
                }
            }
        }
        let text = Text::from_lines(all_lines);
        render_text_to_frame(&text, frame);
    }
}

/// Render a `Text` into a `Frame`, line by line with span styles.
///
/// This is a copy of the private `render_text_to_frame` from
/// `ftui_runtime::string_model`, adapted for our use.
fn render_text_to_frame(text: &Text, frame: &mut Frame) {
    let width = frame.width();
    let height = frame.height();

    for (y, line) in text.lines().iter().enumerate() {
        if y as u16 >= height {
            break;
        }

        let mut x: u16 = 0;
        for span in line.spans() {
            if x >= width {
                break;
            }

            let style = span.style.unwrap_or_default();

            for grapheme in span.content.graphemes(true) {
                if x >= width {
                    break;
                }

                let w = grapheme_width(grapheme);
                if w == 0 {
                    continue;
                }

                // Skip if the wide character would exceed the buffer width
                if x + w as u16 > width {
                    break;
                }

                let content = if w > 1 || grapheme.chars().count() > 1 {
                    let id = frame.intern_with_width(grapheme, w as u8);
                    CellContent::from_grapheme(id)
                } else if let Some(c) = grapheme.chars().next() {
                    CellContent::from_char(c)
                } else {
                    continue;
                };

                let mut cell = Cell::new(content);
                apply_style(&mut cell, style);
                frame.buffer.set(x, y as u16, cell);

                x = x.saturating_add(w as u16);
            }
        }
    }
}

/// Apply a style to a cell.
fn apply_style(cell: &mut Cell, style: ftui_style::Style) {
    if let Some(fg) = style.fg {
        cell.fg = fg;
    }
    if let Some(bg) = style.bg {
        cell.bg = bg;
    }
    if let Some(attrs) = style.attrs {
        let cell_flags: ftui_render::cell::StyleFlags = attrs.into();
        cell.attrs = cell.attrs.with_flags(cell_flags);
    }
}

/// Fix nested [fg=...] and [bg=...] tags by auto-closing before each new open.
///
/// The ftui markup parser does not support nested `[fg]` tags — each opening
/// tag must have a matching close before another can be opened. Our widget code
/// emits colors in an "override" style (like ANSI), where a new `[fg=...]`
/// implicitly replaces the previous color. This function inserts the missing
/// `[/fg]` and `[/bg]` close tags.
///
/// Processes each line independently since markup doesn't span across newlines.
fn fix_nested_tags(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + input.len() / 4);

    for (line_idx, line) in input.split('\n').enumerate() {
        if line_idx > 0 {
            result.push('\n');
        }

        let mut fg_open = false;
        let mut bg_open = false;
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Check for escaped bracket
            if bytes[i] == b'\\' && i + 1 < len && bytes[i + 1] == b'[' {
                result.push('\\');
                result.push('[');
                i += 2;
                continue;
            }

            // Check for tag opening
            if bytes[i] == b'[' {
                // Find end of tag
                if let Some(close_bracket) = line[i..].find(']') {
                    let tag_content = &line[i + 1..i + close_bracket];

                    if tag_content.starts_with("fg=") {
                        // Opening fg tag — close previous if open
                        if fg_open {
                            result.push_str("[/fg]");
                        }
                        result.push_str(&line[i..i + close_bracket + 1]);
                        fg_open = true;
                        i += close_bracket + 1;
                        continue;
                    } else if tag_content == "/fg" {
                        if fg_open {
                            result.push_str("[/fg]");
                            fg_open = false;
                        }
                        // Skip the tag either way (don't emit unmatched close)
                        i += close_bracket + 1;
                        continue;
                    } else if tag_content.starts_with("bg=") {
                        if bg_open {
                            result.push_str("[/bg]");
                        }
                        result.push_str(&line[i..i + close_bracket + 1]);
                        bg_open = true;
                        i += close_bracket + 1;
                        continue;
                    } else if tag_content == "/bg" {
                        if bg_open {
                            result.push_str("[/bg]");
                            bg_open = false;
                        }
                        i += close_bracket + 1;
                        continue;
                    }
                }
            }

            // Regular character
            let ch = line[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }

        // Close any open tags at end of line
        if fg_open {
            result.push_str("[/fg]");
        }
        if bg_open {
            result.push_str("[/bg]");
        }
    }

    result
}
