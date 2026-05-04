use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RichSelectItem {
    pub label: String,
    pub detail: Option<String>,
    pub subtitle: Option<String>,
}

pub(super) fn is_submit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        code,
        KeyCode::Enter | KeyCode::Char('\r') | KeyCode::Char('\n')
    ) || (code == KeyCode::Char('m') && modifiers.contains(KeyModifiers::CONTROL))
        || (code == KeyCode::Char('j') && modifiers.contains(KeyModifiers::CONTROL))
}

pub(super) fn is_actionable_key_event_kind(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

pub(super) fn write_selection_screen_enter<W: Write>(stdout: &mut W) -> io::Result<()> {
    use crossterm::{cursor, queue};

    queue!(
        stdout,
        terminal::EnterAlternateScreen,
        cursor::Hide,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    )
}

fn write_selection_screen_leave<W: Write>(stdout: &mut W) -> io::Result<()> {
    use crossterm::{cursor, queue};

    queue!(stdout, cursor::Show, terminal::LeaveAlternateScreen)
}

struct SelectionScreenGuard;

impl SelectionScreenGuard {
    fn new() -> Result<Self> {
        let mut stdout = io::stdout();
        write_selection_screen_enter(&mut stdout)
            .context("Failed to enter setup selection screen")?;
        stdout.flush()?;
        Ok(Self)
    }
}

impl Drop for SelectionScreenGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = write_selection_screen_leave(&mut stdout);
        let _ = stdout.flush();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectMove {
    Previous,
    Next,
    PageUp(usize),
    PageDown(usize),
    First,
    Last,
}

pub(super) fn move_select_index(selected: usize, item_count: usize, movement: SelectMove) -> usize {
    if item_count == 0 {
        return 0;
    }

    match movement {
        SelectMove::Previous => {
            if selected == 0 {
                item_count.saturating_sub(1)
            } else {
                selected.saturating_sub(1)
            }
        }
        SelectMove::Next => {
            let next = selected.saturating_add(1);
            if next >= item_count {
                0
            } else {
                next
            }
        }
        SelectMove::PageUp(step) => selected.saturating_sub(step.max(1)),
        SelectMove::PageDown(step) => selected
            .saturating_add(step.max(1))
            .min(item_count.saturating_sub(1)),
        SelectMove::First => 0,
        SelectMove::Last => item_count.saturating_sub(1),
    }
}

pub(super) fn select_visible_window_start(
    selected: usize,
    item_count: usize,
    visible_capacity: usize,
) -> usize {
    if item_count <= visible_capacity || visible_capacity == 0 {
        return 0;
    }
    selected
        .saturating_sub(visible_capacity.saturating_sub(1))
        .min(item_count.saturating_sub(visible_capacity))
}

fn select_visible_capacity(
    title: &str,
    terminal_height: u16,
    selected_extra_lines: usize,
) -> usize {
    let title_lines = title.lines().count().max(1);
    let reserved_lines = title_lines
        .saturating_add(3)
        .saturating_add(selected_extra_lines);
    (terminal_height as usize)
        .saturating_sub(reserved_lines)
        .max(1)
}

fn terminal_height() -> u16 {
    terminal::size().map(|(_, height)| height).unwrap_or(24)
}

fn selection_status_line(start: usize, end: usize, total: usize) -> Option<String> {
    (total > end.saturating_sub(start))
        .then(|| format!("    showing {}-{} of {total}", start + 1, end))
}

pub(super) fn select_list(
    title: &str,
    items: &[(&str, &str)],
    allow_esc: bool,
    default_index: usize,
) -> Result<Option<usize>> {
    use crossterm::{cursor, execute, queue};

    let mut stdout = io::stdout();
    let mut selected: usize = default_index.min(items.len().saturating_sub(1));
    let _raw_mode = RawModeGuard::new()?;
    let _mouse_capture = MouseCaptureGuard::new()?;
    let _selection_screen = SelectionScreenGuard::new()?;

    (|| -> Result<Option<usize>> {
        loop {
            let visible_capacity = select_visible_capacity(title, terminal_height(), 0);
            let start = select_visible_window_start(selected, items.len(), visible_capacity);
            let end = start.saturating_add(visible_capacity).min(items.len());
            queue!(
                stdout,
                cursor::MoveTo(0, 0),
                terminal::Clear(terminal::ClearType::All),
                style::SetForegroundColor(style::Color::White),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(title),
                style::SetAttribute(style::Attribute::Reset),
                style::SetForegroundColor(style::Color::Reset),
                style::Print("\r\n\r\n"),
            )?;

            for (i, (label, desc)) in items.iter().enumerate().skip(start).take(end - start) {
                if i == selected {
                    let mut line = format!("  > {label}");
                    if !desc.is_empty() {
                        line.push_str(&format!(" ({desc})"));
                    }
                    queue!(
                        stdout,
                        style::SetForegroundColor(style::Color::Green),
                        style::SetAttribute(style::Attribute::Bold),
                        style::Print(&line),
                        style::SetAttribute(style::Attribute::Reset),
                        style::SetForegroundColor(style::Color::Reset),
                        style::Print("\r\n"),
                    )?;
                } else {
                    let mut line = format!("    {label}");
                    if !desc.is_empty() {
                        line.push_str(&format!(" ({desc})"));
                    }
                    queue!(
                        stdout,
                        style::SetForegroundColor(style::Color::Grey),
                        style::Print(&line),
                        style::SetForegroundColor(style::Color::Reset),
                        style::Print("\r\n"),
                    )?;
                }
            }
            if let Some(status) = selection_status_line(start, end, items.len()) {
                queue!(
                    stdout,
                    style::SetForegroundColor(style::Color::DarkGrey),
                    style::Print(status),
                    style::SetForegroundColor(style::Color::Reset),
                    style::Print("\r\n"),
                )?;
            }

            stdout.flush()?;

            match event::read()? {
                Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                    ..
                }) if is_actionable_key_event_kind(kind) => match code {
                    KeyCode::Up => {
                        selected = move_select_index(selected, items.len(), SelectMove::Previous);
                    }
                    KeyCode::Down => {
                        selected = move_select_index(selected, items.len(), SelectMove::Next);
                    }
                    KeyCode::PageUp => {
                        selected = move_select_index(
                            selected,
                            items.len(),
                            SelectMove::PageUp(visible_capacity),
                        );
                    }
                    KeyCode::PageDown => {
                        selected = move_select_index(
                            selected,
                            items.len(),
                            SelectMove::PageDown(visible_capacity),
                        );
                    }
                    KeyCode::Home => {
                        selected = move_select_index(selected, items.len(), SelectMove::First);
                    }
                    KeyCode::End => {
                        selected = move_select_index(selected, items.len(), SelectMove::Last);
                    }
                    _ if is_submit_key(code, modifiers) => {
                        execute!(stdout, style::SetForegroundColor(style::Color::Reset),)?;
                        return Ok(Some(selected));
                    }
                    KeyCode::Esc if allow_esc => {
                        execute!(stdout, style::SetForegroundColor(style::Color::Reset),)?;
                        return Ok(None);
                    }
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        anyhow::bail!("Setup cancelled by user");
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollUp => {
                        selected = move_select_index(selected, items.len(), SelectMove::Previous);
                    }
                    crossterm::event::MouseEventKind::ScrollDown => {
                        selected = move_select_index(selected, items.len(), SelectMove::Next);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    })()
}

pub(super) fn select_rich_list(
    title: &str,
    items: &[RichSelectItem],
    allow_esc: bool,
    default_index: usize,
) -> Result<Option<usize>> {
    use crossterm::{cursor, execute, queue};

    let mut stdout = io::stdout();
    let mut selected: usize = default_index.min(items.len().saturating_sub(1));
    let _raw_mode = RawModeGuard::new()?;
    let _mouse_capture = MouseCaptureGuard::new()?;
    let _selection_screen = SelectionScreenGuard::new()?;

    (|| -> Result<Option<usize>> {
        loop {
            let selected_has_subtitle = items
                .get(selected)
                .and_then(|item| item.subtitle.as_deref())
                .is_some_and(|subtitle| !subtitle.is_empty());
            let visible_capacity = select_visible_capacity(
                title,
                terminal_height(),
                usize::from(selected_has_subtitle),
            );
            let start = select_visible_window_start(selected, items.len(), visible_capacity);
            let end = start.saturating_add(visible_capacity).min(items.len());
            queue!(
                stdout,
                cursor::MoveTo(0, 0),
                terminal::Clear(terminal::ClearType::All),
                style::SetForegroundColor(style::Color::White),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(title),
                style::SetAttribute(style::Attribute::Reset),
                style::SetForegroundColor(style::Color::Reset),
                style::Print("\r\n\r\n"),
            )?;

            for (i, item) in items.iter().enumerate().skip(start).take(end - start) {
                let mut line = if i == selected {
                    format!("  > {}", item.label)
                } else {
                    format!("    {}", item.label)
                };
                if let Some(detail) = item.detail.as_deref().filter(|detail| !detail.is_empty()) {
                    line.push_str(&format!(" ({detail})"));
                }

                if i == selected {
                    queue!(
                        stdout,
                        style::SetForegroundColor(style::Color::Green),
                        style::SetAttribute(style::Attribute::Bold),
                        style::Print(&line),
                        style::SetAttribute(style::Attribute::Reset),
                        style::SetForegroundColor(style::Color::Reset),
                        style::Print("\r\n"),
                    )?;
                } else {
                    queue!(
                        stdout,
                        style::SetForegroundColor(style::Color::Grey),
                        style::Print(&line),
                        style::SetForegroundColor(style::Color::Reset),
                        style::Print("\r\n"),
                    )?;
                }

                if i == selected {
                    if let Some(subtitle) = item
                        .subtitle
                        .as_deref()
                        .filter(|subtitle| !subtitle.is_empty())
                    {
                        queue!(
                            stdout,
                            style::SetForegroundColor(style::Color::DarkGrey),
                            style::Print(format!("      {subtitle}")),
                            style::SetForegroundColor(style::Color::Reset),
                            style::Print("\r\n"),
                        )?;
                    }
                }
            }
            if let Some(status) = selection_status_line(start, end, items.len()) {
                queue!(
                    stdout,
                    style::SetForegroundColor(style::Color::DarkGrey),
                    style::Print(status),
                    style::SetForegroundColor(style::Color::Reset),
                    style::Print("\r\n"),
                )?;
            }

            stdout.flush()?;

            match event::read()? {
                Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                    ..
                }) if is_actionable_key_event_kind(kind) => match code {
                    KeyCode::Up => {
                        selected = move_select_index(selected, items.len(), SelectMove::Previous);
                    }
                    KeyCode::Down => {
                        selected = move_select_index(selected, items.len(), SelectMove::Next);
                    }
                    KeyCode::PageUp => {
                        selected = move_select_index(
                            selected,
                            items.len(),
                            SelectMove::PageUp(visible_capacity),
                        );
                    }
                    KeyCode::PageDown => {
                        selected = move_select_index(
                            selected,
                            items.len(),
                            SelectMove::PageDown(visible_capacity),
                        );
                    }
                    KeyCode::Home => {
                        selected = move_select_index(selected, items.len(), SelectMove::First);
                    }
                    KeyCode::End => {
                        selected = move_select_index(selected, items.len(), SelectMove::Last);
                    }
                    _ if is_submit_key(code, modifiers) => {
                        execute!(stdout, style::SetForegroundColor(style::Color::Reset),)?;
                        return Ok(Some(selected));
                    }
                    KeyCode::Esc if allow_esc => {
                        execute!(stdout, style::SetForegroundColor(style::Color::Reset),)?;
                        return Ok(None);
                    }
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        anyhow::bail!("Setup cancelled by user");
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollUp => {
                        selected = move_select_index(selected, items.len(), SelectMove::Previous);
                    }
                    crossterm::event::MouseEventKind::ScrollDown => {
                        selected = move_select_index(selected, items.len(), SelectMove::Next);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    })()
}

pub(super) fn text_input(prompt_text: &str, default: &str, masked: bool) -> Result<Option<String>> {
    use crossterm::execute;

    let mut stdout = io::stdout();
    if !default.is_empty() {
        execute!(stdout, style::Print(format!("{prompt_text} [{default}]: ")))?;
    } else {
        execute!(stdout, style::Print(format!("{prompt_text}: ")))?;
    }

    let _raw_mode = RawModeGuard::new()?;

    (|| -> Result<Option<String>> {
        let mut input = String::new();
        loop {
            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) = event::read()?
            {
                if is_actionable_key_event_kind(kind) {
                    match code {
                        _ if is_submit_key(code, modifiers) => {
                            execute!(stdout, style::Print("\r\n"))?;
                            let value = if input.is_empty() && !default.is_empty() {
                                default.to_string()
                            } else {
                                input
                            };
                            return Ok(Some(value));
                        }
                        KeyCode::Esc => {
                            execute!(stdout, style::Print("\r\n"))?;
                            return Ok(None);
                        }
                        KeyCode::Backspace => {
                            if input.pop().is_some() {
                                execute!(stdout, style::Print("\x08 \x08"))?;
                            }
                        }
                        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                            anyhow::bail!("Setup cancelled by user");
                        }
                        KeyCode::Char(c) => {
                            input.push(c);
                            if masked {
                                execute!(stdout, style::Print("*"))?;
                            } else {
                                execute!(stdout, style::Print(format!("{c}")))?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    })()
}
