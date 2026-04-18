use super::*;

use base64::Engine;
use std::process::{Command, Stdio};

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

        let path = std::path::Path::new(&expanded);
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| expanded.clone());
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        let inferred_kind = match extension.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "svg" => {
                Some(("image", infer_attachment_mime(path)))
            }
            "mp3" | "wav" | "ogg" | "m4a" | "mp4" | "flac" | "webm" => {
                Some(("audio", infer_attachment_mime(path)))
            }
            _ => None,
        };

        let attachment = match inferred_kind {
            Some((kind, mime_type)) => match std::fs::read(&expanded) {
                Ok(bytes) => {
                    let data_url = format!(
                        "data:{};base64,{}",
                        mime_type,
                        base64::engine::general_purpose::STANDARD.encode(&bytes)
                    );
                    let block = serde_json::json!({
                        "type": kind,
                        "data_url": data_url,
                        "mime_type": mime_type,
                    });
                    Some(Attachment {
                        filename: filename.clone(),
                        size_bytes: bytes.len(),
                        payload: AttachmentPayload::ContentBlock(block),
                    })
                }
                Err(error) => {
                    self.status_line = "Attach failed".to_string();
                    self.last_error = Some(format!("Attach failed: {}", error));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                    None
                }
            },
            None => match std::fs::read_to_string(&expanded) {
                Ok(content) => Some(Attachment {
                    filename: filename.clone(),
                    size_bytes: content.len(),
                    payload: AttachmentPayload::Text(content),
                }),
                Err(error) => {
                    self.status_line = "Attach failed".to_string();
                    self.last_error = Some(format!(
                        "Attach failed: text attachments must be UTF-8 or use a supported image/audio format ({})",
                        error
                    ));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                    None
                }
            },
        };

        if let Some(attachment) = attachment {
            let size = attachment.size_bytes;
            self.attachments.push(attachment);
            self.status_line = format!("Attached: {} ({} bytes)", filename, size);
        }
    }

    pub(super) fn start_voice_capture(&mut self) {
        if self.voice_recording {
            self.status_line = "Voice capture already active".to_string();
            return;
        }

        let capture_path =
            std::env::temp_dir().join(format!("tamux-voice-{}.wav", uuid::Uuid::new_v4()));
        let capture_path_string = capture_path.to_string_lossy().to_string();

        let ffmpeg = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "alsa",
                "-i",
                "default",
                "-t",
                "10",
                "-ac",
                "1",
                &capture_path_string,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let child = match ffmpeg {
            Ok(child) => Ok(child),
            Err(_) => Command::new("arecord")
                .args(["-f", "cd", "-t", "wav", "-d", "10", &capture_path_string])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn(),
        };

        match child {
            Ok(child) => {
                self.voice_recorder = Some(child);
                self.voice_recording = true;
                self.voice_capture_path = Some(capture_path_string);
                self.status_line = "Voice capture started".to_string();
                self.show_input_notice(
                    "Recording voice capture... toggle again to stop",
                    InputNoticeKind::Success,
                    120,
                    false,
                );
            }
            Err(error) => {
                self.status_line = "Voice capture failed (ffmpeg/arecord unavailable)".to_string();
                self.show_input_notice(
                    "Voice capture failed: install ffmpeg or arecord",
                    InputNoticeKind::Warning,
                    90,
                    true,
                );
                self.last_error = Some(format!(
                    "Voice capture failed: ffmpeg/arecord unavailable ({error})"
                ));
                self.error_active = true;
                self.error_tick = self.tick_counter;
            }
        }
    }

    pub(super) fn stop_voice_capture(&mut self) -> Option<String> {
        if let Some(mut child) = self.voice_recorder.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        self.voice_recording = false;
        let capture_path = self.voice_capture_path.take();
        if let Some(path) = capture_path.as_ref() {
            if std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) == 0 {
                self.status_line = "Voice capture empty".to_string();
                self.show_input_notice(
                    "Voice capture was empty — try speaking louder/longer",
                    InputNoticeKind::Warning,
                    80,
                    true,
                );
                return None;
            }
            self.status_line = "Voice capture stopped".to_string();
        }
        capture_path
    }

    pub(super) fn stop_voice_playback(&mut self) {
        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.status_line = "Audio playback stopped".to_string();
            self.show_input_notice("Stopped playback", InputNoticeKind::Success, 50, true);
        } else {
            self.status_line = "No active audio playback".to_string();
        }
    }

    pub(super) fn play_audio_path(&mut self, path: &str) {
        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        let mpv = Command::new("mpv")
            .args(["--no-video", path])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let child = match mpv {
            Ok(child) => Ok(child),
            Err(_) => Command::new("paplay")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn(),
        };

        match child {
            Ok(child) => {
                self.voice_player = Some(child);
                self.status_line = "Playing synthesized speech...".to_string();
            }
            Err(error) => {
                self.status_line = "Audio playback failed (mpv/paplay unavailable)".to_string();
                self.show_input_notice(
                    "Audio playback failed: install mpv or paplay",
                    InputNoticeKind::Warning,
                    90,
                    true,
                );
                self.last_error = Some(format!(
                    "Audio playback failed: mpv/paplay unavailable ({error})"
                ));
                self.error_active = true;
                self.error_tick = self.tick_counter;
            }
        }
    }
}

fn infer_attachment_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
}
