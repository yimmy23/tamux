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

    (|| -> Result<Option<usize>> {
        loop {
            queue!(
                stdout,
                style::SetForegroundColor(style::Color::White),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(title),
                style::SetAttribute(style::Attribute::Reset),
                style::SetForegroundColor(style::Color::Reset),
                style::Print("\r\n\r\n"),
            )?;

            for (i, (label, desc)) in items.iter().enumerate() {
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

            stdout.flush()?;

            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) = event::read()?
            {
                if is_actionable_key_event_kind(kind) {
                    match code {
                        KeyCode::Up => {
                            if selected == 0 {
                                selected = items.len().saturating_sub(1);
                            } else {
                                selected -= 1;
                            }
                        }
                        KeyCode::Down => {
                            selected += 1;
                            if selected >= items.len() {
                                selected = 0;
                            }
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
                    }
                }
            }

            let lines_to_clear = items.len() + 2;
            execute!(
                stdout,
                cursor::MoveUp(lines_to_clear as u16),
                terminal::Clear(terminal::ClearType::FromCursorDown),
            )?;
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

    (|| -> Result<Option<usize>> {
        loop {
            queue!(
                stdout,
                style::SetForegroundColor(style::Color::White),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(title),
                style::SetAttribute(style::Attribute::Reset),
                style::SetForegroundColor(style::Color::Reset),
                style::Print("\r\n\r\n"),
            )?;

            for (i, item) in items.iter().enumerate() {
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

            stdout.flush()?;

            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) = event::read()?
            {
                if is_actionable_key_event_kind(kind) {
                    match code {
                        KeyCode::Up => {
                            if selected == 0 {
                                selected = items.len().saturating_sub(1);
                            } else {
                                selected -= 1;
                            }
                        }
                        KeyCode::Down => {
                            selected += 1;
                            if selected >= items.len() {
                                selected = 0;
                            }
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
                    }
                }
            }

            let lines_to_clear = title.lines().count()
                + 1
                + items
                    .iter()
                    .map(|item| 1 + usize::from(item.subtitle.is_some()))
                    .sum::<usize>();
            execute!(
                stdout,
                cursor::MoveUp(lines_to_clear as u16),
                terminal::Clear(terminal::ClearType::FromCursorDown),
            )?;
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
