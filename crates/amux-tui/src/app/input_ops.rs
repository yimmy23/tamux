use super::*;

impl TuiModel {
    pub(super) fn input_wrap_width(&self) -> usize {
        self.width.saturating_sub(4).max(1) as usize
    }

    pub(super) fn input_height(&self) -> u16 {
        let inner_w = self.input_wrap_width();
        if inner_w <= 2 {
            return 3;
        }

        let wrapped = self.input.wrapped_display_buffer(inner_w);
        let attach_count = self.attachments.len();
        let min_visual_lines = if wrapped.is_empty() {
            1
        } else {
            wrapped.split('\n').count().max(1)
        };
        (min_visual_lines + attach_count + 2).clamp(3, 12) as u16
    }

    pub fn handle_paste(&mut self, text: String) {
        if let Some(modal_kind) = self.modal.top() {
            match modal_kind {
                modal::ModalKind::Settings if self.auth.login_target.is_some() => {
                    for ch in text.chars() {
                        match ch {
                            '\r' | '\n' => {}
                            other => self
                                .auth
                                .reduce(crate::state::auth::AuthAction::LoginKeyChar(other)),
                        }
                    }
                    return;
                }
                modal::ModalKind::Settings if self.settings.is_editing() => {
                    let allow_newlines = self.settings.is_textarea();
                    for ch in text.chars() {
                        match ch {
                            '\r' => {}
                            '\n' if allow_newlines => {
                                self.settings.reduce(SettingsAction::InsertChar('\n'));
                            }
                            '\n' => {}
                            other => self.settings.reduce(SettingsAction::InsertChar(other)),
                        }
                    }
                    return;
                }
                modal::ModalKind::CommandPalette
                | modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker => {
                    self.input.reduce(input::InputAction::Clear);
                    for ch in text.chars() {
                        match ch {
                            '\r' | '\n' => {}
                            other => self.input.reduce(input::InputAction::InsertChar(other)),
                        }
                    }
                    self.modal.reduce(modal::ModalAction::SetQuery(
                        self.input.buffer().to_string(),
                    ));
                    if modal_kind == modal::ModalKind::ThreadPicker {
                        self.sync_thread_picker_item_count();
                    } else if modal_kind == modal::ModalKind::GoalPicker {
                        self.sync_goal_picker_item_count();
                    }
                    return;
                }
                _ => return,
            }
        }

        if self.focus != FocusArea::Input {
            self.focus = FocusArea::Input;
            self.input.set_mode(input::InputMode::Insert);
        }

        let trimmed = text.trim();
        if !trimmed.contains('\n')
            && (trimmed.starts_with('/')
                || trimmed.starts_with('~')
                || trimmed.starts_with("C:\\")
                || trimmed.starts_with("D:\\"))
        {
            let expanded = if trimmed.starts_with('~') {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_default();
                trimmed.replacen('~', &home, 1)
            } else {
                trimmed.to_string()
            };

            if std::path::Path::new(&expanded).is_file() {
                self.attach_file(trimmed);
                return;
            }
        }

        if text.contains('\n') {
            self.input.insert_paste_block(text);
        } else {
            for c in text.chars() {
                self.input.reduce(input::InputAction::InsertChar(c));
            }
        }
    }

    pub(super) fn attach_file(&mut self, path: &str) {
        let expanded = if path.starts_with('~') {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default();
            path.replacen('~', &home, 1)
        } else {
            path.to_string()
        };

        match std::fs::read_to_string(&expanded) {
            Ok(content) => {
                let size = content.len();
                let filename = std::path::Path::new(&expanded)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| expanded.clone());
                self.attachments.push(Attachment {
                    filename: filename.clone(),
                    content,
                    size_bytes: size,
                });
                self.status_line = format!("Attached: {} ({} bytes)", filename, size);
            }
            Err(e) => {
                self.status_line = "Attach failed".to_string();
                self.last_error = Some(format!("Attach failed: {}", e));
                self.error_active = true;
                self.error_tick = self.tick_counter;
            }
        }
    }
}
