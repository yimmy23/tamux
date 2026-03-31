use super::*;

impl InputState {
    fn placeholder_at(buffer: &str, start: usize) -> Option<(usize, usize)> {
        if !buffer[start..].starts_with("\x00PASTE:") {
            return None;
        }
        let rest = &buffer[start + 1..];
        let end_rel = rest.find('\x00')?;
        let end = start + 1 + end_rel + 1;
        let tag = &rest[..end_rel];
        let id = tag.strip_prefix("PASTE:")?.parse::<usize>().ok()?;
        Some((end, id))
    }

    fn byte_offset_for_char_index(text: &str, char_idx: usize) -> usize {
        text.char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    pub fn display_buffer_and_cursor(&self) -> (String, usize) {
        let raw = self.buffer_cache.as_str();
        let raw_cursor = self.cursor_pos().min(raw.len());
        let mut display = String::new();
        let mut raw_idx = 0usize;
        let mut display_cursor = None;

        while raw_idx < raw.len() {
            if display_cursor.is_none() && raw_cursor == raw_idx {
                display_cursor = Some(display.len());
            }

            if let Some((end, id)) = Self::placeholder_at(raw, raw_idx) {
                let label = Self::paste_block_display(id, &self.paste_blocks)
                    .unwrap_or_else(|| raw[raw_idx..end].to_string());

                if display_cursor.is_none() && raw_cursor > raw_idx && raw_cursor < end {
                    let raw_progress = raw_cursor - raw_idx;
                    let raw_len = (end - raw_idx).max(1);
                    let label_chars = label.chars().count();
                    let mapped_chars = ((raw_progress * label_chars) + (raw_len / 2)) / raw_len;
                    display_cursor = Some(
                        display.len() + Self::byte_offset_for_char_index(&label, mapped_chars),
                    );
                }

                display.push_str(&label);
                raw_idx = end;
                continue;
            }

            let ch = raw[raw_idx..]
                .chars()
                .next()
                .expect("raw_idx always points to a char boundary");
            display.push(ch);
            raw_idx += ch.len_utf8();
        }

        let display_cursor = display_cursor.unwrap_or(display.len());
        (display, display_cursor)
    }

    pub fn display_buffer(&self) -> String {
        self.display_buffer_and_cursor().0
    }

    pub fn display_offset_to_buffer_offset(&self, display_offset: usize) -> usize {
        let raw = self.buffer_cache.as_str();
        let mut raw_idx = 0usize;
        let mut display_idx = 0usize;
        let target = display_offset.min(self.display_buffer().len());

        while raw_idx < raw.len() {
            if let Some((end, id)) = Self::placeholder_at(raw, raw_idx) {
                let label = Self::paste_block_display(id, &self.paste_blocks)
                    .unwrap_or_else(|| raw[raw_idx..end].to_string());
                let label_len = label.len();
                if target <= display_idx + label_len {
                    let rel = target.saturating_sub(display_idx);
                    let rel_chars = label[..rel.min(label.len())].chars().count();
                    let total_chars = label.chars().count().max(1);
                    let raw_len = end - raw_idx;
                    let mapped = ((rel_chars * raw_len) + (total_chars / 2)) / total_chars;
                    return raw_idx + mapped.min(raw_len);
                }
                display_idx += label_len;
                raw_idx = end;
                continue;
            }

            let ch = raw[raw_idx..]
                .chars()
                .next()
                .expect("raw_idx always points to a char boundary");
            let ch_len = ch.len_utf8();
            if target <= display_idx + ch_len {
                return if target <= display_idx {
                    raw_idx
                } else {
                    raw_idx + ch_len
                };
            }
            display_idx += ch_len;
            raw_idx += ch_len;
        }

        raw.len()
    }

    fn byte_len_for_display_width(text: &str, max_width: usize) -> usize {
        if max_width == 0 {
            return 0;
        }

        let mut used = 0usize;
        let mut end = 0usize;
        for (idx, ch) in text.char_indices() {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + width > max_width {
                return if end == 0 { idx + ch.len_utf8() } else { end };
            }
            used += width;
            end = idx + ch.len_utf8();
        }
        text.len()
    }

    pub fn wrapped_display_buffer_and_cursor(&self, wrap_width: usize) -> (String, usize) {
        let (display, cursor) = self.display_buffer_and_cursor();
        if wrap_width == 0 {
            return (display, cursor);
        }

        let mut wrapped = String::new();
        let mut wrapped_cursor = None;
        let lines: Vec<&str> = display.split('\n').collect();
        let mut display_offset = 0usize;

        for (line_idx, line) in lines.iter().enumerate() {
            let mut remaining = *line;
            let mut line_offset = 0usize;

            loop {
                if wrapped_cursor.is_none() && cursor == display_offset + line_offset {
                    wrapped_cursor = Some(wrapped.len());
                }

                let segment_len = if remaining.is_empty() {
                    0
                } else {
                    Self::byte_len_for_display_width(remaining, wrap_width)
                };
                let segment = &remaining[..segment_len.min(remaining.len())];

                if wrapped_cursor.is_none()
                    && cursor > display_offset + line_offset
                    && cursor <= display_offset + line_offset + segment.len()
                {
                    wrapped_cursor = Some(wrapped.len() + cursor - (display_offset + line_offset));
                }

                wrapped.push_str(segment);
                line_offset += segment.len();

                if line_offset >= line.len() {
                    break;
                }

                wrapped.push('\n');
                remaining = &remaining[segment.len()..];
            }

            display_offset += line.len();
            if line_idx + 1 < lines.len() {
                if wrapped_cursor.is_none() && cursor == display_offset {
                    wrapped_cursor = Some(wrapped.len());
                }
                wrapped.push('\n');
                display_offset += 1;
            }
        }

        let wrapped_cursor = wrapped_cursor.unwrap_or(wrapped.len());
        (wrapped, wrapped_cursor)
    }

    pub fn wrapped_display_buffer(&self, wrap_width: usize) -> String {
        self.wrapped_display_buffer_and_cursor(wrap_width).0
    }

    pub fn wrapped_display_offset_to_buffer_offset(
        &self,
        wrapped_offset: usize,
        wrap_width: usize,
    ) -> usize {
        let display = self.display_buffer();
        if wrap_width == 0 {
            return self.display_offset_to_buffer_offset(wrapped_offset);
        }

        let mut wrapped_idx = 0usize;
        let mut display_idx = 0usize;
        let lines: Vec<&str> = display.split('\n').collect();

        for (line_idx, line) in lines.iter().enumerate() {
            let mut remaining = *line;
            let mut line_offset = 0usize;

            loop {
                let segment_len = if remaining.is_empty() {
                    0
                } else {
                    Self::byte_len_for_display_width(remaining, wrap_width)
                };

                if wrapped_offset <= wrapped_idx + segment_len {
                    let rel = wrapped_offset.saturating_sub(wrapped_idx).min(segment_len);
                    return self.display_offset_to_buffer_offset(display_idx + line_offset + rel);
                }

                wrapped_idx += segment_len;
                line_offset += segment_len;

                if line_offset >= line.len() {
                    break;
                }

                if wrapped_offset == wrapped_idx {
                    return self.display_offset_to_buffer_offset(display_idx + line_offset);
                }
                wrapped_idx += 1;
                remaining = &remaining[segment_len..];
            }

            display_idx += line.len();
            if line_idx + 1 < lines.len() {
                if wrapped_offset <= wrapped_idx {
                    return self.display_offset_to_buffer_offset(display_idx);
                }
                wrapped_idx += 1;
                display_idx += 1;
            }
        }

        self.buffer_cache.len()
    }

}
