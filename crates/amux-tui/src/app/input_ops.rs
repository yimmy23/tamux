use super::*;

use base64::Engine;
use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoiceCaptureBackend {
    Pulse,
    Alsa,
}

fn preferred_voice_capture_backend_from_env(has_pulse_server: bool) -> VoiceCaptureBackend {
    if has_pulse_server {
        VoiceCaptureBackend::Pulse
    } else {
        VoiceCaptureBackend::Alsa
    }
}

fn preferred_voice_capture_backend() -> VoiceCaptureBackend {
    preferred_voice_capture_backend_from_env(std::env::var_os("PULSE_SERVER").is_some())
}

fn ffmpeg_voice_capture_args(capture_path: &str, backend: VoiceCaptureBackend) -> Vec<String> {
    let input_format = match backend {
        VoiceCaptureBackend::Pulse => "pulse",
        VoiceCaptureBackend::Alsa => "alsa",
    };
    vec![
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-f".to_string(),
        input_format.to_string(),
        "-i".to_string(),
        "default".to_string(),
        "-t".to_string(),
        "10".to_string(),
        "-ac".to_string(),
        "1".to_string(),
        capture_path.to_string(),
    ]
}

fn voice_capture_backend_label(backend: VoiceCaptureBackend) -> &'static str {
    match backend {
        VoiceCaptureBackend::Pulse => "ffmpeg/pulse",
        VoiceCaptureBackend::Alsa => "ffmpeg/alsa",
    }
}

fn read_voice_capture_error_summary(stderr_path: Option<&str>) -> Option<String> {
    let path = stderr_path?;
    let content = std::fs::read_to_string(path).ok()?;
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(240).collect::<String>())
}

fn spawn_ffmpeg_voice_capture(
    capture_path: &str,
    stderr_path: &Path,
    backend: VoiceCaptureBackend,
) -> std::io::Result<std::process::Child> {
    let stderr = File::create(stderr_path)?;
    Command::new("ffmpeg")
        .args(ffmpeg_voice_capture_args(capture_path, backend))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
}

fn spawn_arecord_voice_capture(
    capture_path: &str,
    stderr_path: &Path,
) -> std::io::Result<std::process::Child> {
    let stderr = File::create(stderr_path)?;
    Command::new("arecord")
        .args(["-f", "cd", "-t", "wav", "-d", "10", capture_path])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
}

fn stop_voice_capture_process(child: &mut Child) -> Option<ExitStatus> {
    #[cfg(unix)]
    {
        let interrupt_result = unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
        if interrupt_result == 0 {
            let deadline = Instant::now() + Duration::from_millis(1500);
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return Some(status),
                    Ok(None) if Instant::now() < deadline => sleep(Duration::from_millis(50)),
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }

    let _ = child.kill();
    child.wait().ok()
}

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
                modal::ModalKind::CommandPalette => {
                    let query = text
                        .chars()
                        .filter(|ch| !matches!(ch, '\r' | '\n'))
                        .collect::<String>();
                    self.modal.reduce(modal::ModalAction::SetQuery(query));
                    return;
                }
                modal::ModalKind::WorkspaceCreateTask => {
                    self.paste_into_workspace_create_modal(&text);
                    return;
                }
                modal::ModalKind::WorkspaceEditTask => {
                    self.paste_into_workspace_edit_modal(&text);
                    return;
                }
                modal::ModalKind::ThreadPicker | modal::ModalKind::GoalPicker => {
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
        self.sync_goal_mission_control_prompt_from_input();
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

        let capture_id = uuid::Uuid::new_v4();
        let capture_path = std::env::temp_dir().join(format!("tamux-voice-{capture_id}.wav"));
        let stderr_path = std::env::temp_dir().join(format!("tamux-voice-{capture_id}.log"));
        let capture_path_string = capture_path.to_string_lossy().to_string();
        let stderr_path_string = stderr_path.to_string_lossy().to_string();
        let ffmpeg_backend = preferred_voice_capture_backend();

        let child =
            match spawn_ffmpeg_voice_capture(&capture_path_string, &stderr_path, ffmpeg_backend) {
                Ok(child) => Ok((
                    child,
                    voice_capture_backend_label(ffmpeg_backend).to_string(),
                )),
                Err(_) => spawn_arecord_voice_capture(&capture_path_string, &stderr_path)
                    .map(|child| (child, "arecord".to_string())),
            };

        match child {
            Ok((child, backend_label)) => {
                self.voice_recorder = Some(child);
                self.voice_recording = true;
                self.voice_capture_path = Some(capture_path_string);
                self.voice_capture_stderr_path = Some(stderr_path_string);
                self.voice_capture_backend_label = Some(backend_label);
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
        let recorder_status = self
            .voice_recorder
            .as_mut()
            .and_then(stop_voice_capture_process);
        self.voice_recorder.take();
        let stderr_path = self.voice_capture_stderr_path.take();
        let backend_label = self.voice_capture_backend_label.take();

        self.voice_recording = false;
        let capture_path = self.voice_capture_path.take();
        if let Some(path) = capture_path.as_ref() {
            if std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) == 0 {
                if recorder_status.is_some_and(|status| !status.success()) {
                    self.status_line = "Voice capture failed".to_string();
                    self.show_input_notice(
                        "Voice capture failed — check error viewer",
                        InputNoticeKind::Warning,
                        80,
                        true,
                    );
                    let backend = backend_label
                        .map(|label| format!(" ({label})"))
                        .unwrap_or_default();
                    let detail = read_voice_capture_error_summary(stderr_path.as_deref())
                        .unwrap_or_else(|| "recorder exited before writing audio".to_string());
                    self.last_error = Some(format!("Voice capture failed{backend}: {detail}"));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                    if let Some(stderr_path) = stderr_path.as_deref() {
                        let _ = std::fs::remove_file(stderr_path);
                    }
                    return None;
                }
                self.status_line = "Voice capture empty".to_string();
                self.show_input_notice(
                    "Voice capture was empty — try speaking louder/longer",
                    InputNoticeKind::Warning,
                    80,
                    true,
                );
                if let Some(stderr_path) = stderr_path.as_deref() {
                    let _ = std::fs::remove_file(stderr_path);
                }
                return None;
            }
            self.status_line = "Voice capture stopped".to_string();
        }
        if let Some(stderr_path) = stderr_path.as_deref() {
            let _ = std::fs::remove_file(stderr_path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_voice_capture_backend_uses_pulse_when_pulse_server_is_present() {
        assert_eq!(
            preferred_voice_capture_backend_from_env(true),
            VoiceCaptureBackend::Pulse
        );
    }

    #[test]
    fn preferred_voice_capture_backend_defaults_to_alsa_without_pulse_server() {
        assert_eq!(
            preferred_voice_capture_backend_from_env(false),
            VoiceCaptureBackend::Alsa
        );
    }

    #[test]
    fn ffmpeg_voice_capture_args_use_pulse_input_when_pulse_backend_selected() {
        let args = ffmpeg_voice_capture_args("/tmp/test.wav", VoiceCaptureBackend::Pulse);
        assert_eq!(
            args,
            vec![
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "pulse",
                "-i",
                "default",
                "-t",
                "10",
                "-ac",
                "1",
                "/tmp/test.wav",
            ]
        );
    }

    #[test]
    fn ffmpeg_voice_capture_args_use_alsa_input_when_alsa_backend_selected() {
        let args = ffmpeg_voice_capture_args("/tmp/test.wav", VoiceCaptureBackend::Alsa);
        assert_eq!(args[5], "alsa");
    }
}
