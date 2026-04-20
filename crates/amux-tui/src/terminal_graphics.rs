use std::io::{self, Write};
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalImageProtocol {
    None,
    Kitty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalImageOverlaySpec {
    pub(crate) path: String,
    pub(crate) column: u16,
    pub(crate) row: u16,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

pub(crate) struct TerminalGraphicsRenderer {
    protocol: TerminalImageProtocol,
    last_spec: Option<TerminalImageOverlaySpec>,
}

impl TerminalGraphicsRenderer {
    pub(crate) fn new(protocol: TerminalImageProtocol) -> Self {
        Self {
            protocol,
            last_spec: None,
        }
    }

    pub(crate) fn render(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
        spec: Option<TerminalImageOverlaySpec>,
    ) -> io::Result<()> {
        if self.protocol == TerminalImageProtocol::None {
            self.last_spec = None;
            return Ok(());
        }

        let sequence = match (&self.last_spec, &spec) {
            (Some(previous), Some(current)) if previous == current => None,
            (Some(_), Some(current)) => Some(build_kitty_display_sequence(current, true)),
            (None, Some(current)) => Some(build_kitty_display_sequence(current, false)),
            (Some(_), None) => Some(build_kitty_clear_sequence()),
            (None, None) => None,
        };

        if let Some(sequence) = sequence {
            let sequence = if inside_tmux() {
                wrap_for_tmux_passthrough(&sequence)
            } else {
                sequence
            };
            let backend = terminal.backend_mut();
            backend.write_all(sequence.as_bytes())?;
            backend.flush()?;
        }

        self.last_spec = spec;
        Ok(())
    }
}

pub(crate) fn configure_detected_protocol() -> TerminalImageProtocol {
    let protocol = detect_protocol();
    set_active_protocol(protocol);
    protocol
}

pub(crate) fn active_protocol() -> TerminalImageProtocol {
    *protocol_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn set_active_protocol(protocol: TerminalImageProtocol) {
    *protocol_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = protocol;
}

pub(crate) fn detect_protocol() -> TerminalImageProtocol {
    detect_protocol_from_env(std::env::vars())
}

fn inside_tmux() -> bool {
    detect_tmux_from_env(std::env::vars())
}

fn protocol_cell() -> &'static Mutex<TerminalImageProtocol> {
    static ACTIVE_PROTOCOL: OnceLock<Mutex<TerminalImageProtocol>> = OnceLock::new();
    ACTIVE_PROTOCOL.get_or_init(|| Mutex::new(TerminalImageProtocol::None))
}

fn detect_protocol_from_env<I, K, V>(env: I) -> TerminalImageProtocol
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut force = None;
    let mut term = None;
    let mut term_program = None;
    let mut kitty_window_id = None;

    for (key, value) in env {
        let key = key.as_ref();
        let value = value.as_ref();
        match key {
            "TAMUX_TUI_IMAGE_PROTOCOL" | "AMUX_TUI_IMAGE_PROTOCOL" => {
                force = Some(value.to_string())
            }
            "TERM" => term = Some(value.to_ascii_lowercase()),
            "TERM_PROGRAM" => term_program = Some(value.to_ascii_lowercase()),
            "KITTY_WINDOW_ID" => kitty_window_id = Some(value.to_string()),
            _ => {}
        }
    }

    if let Some(force) = force {
        return match force.trim().to_ascii_lowercase().as_str() {
            "kitty" => TerminalImageProtocol::Kitty,
            "none" | "off" | "0" | "false" => TerminalImageProtocol::None,
            _ => TerminalImageProtocol::None,
        };
    }

    if kitty_window_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return TerminalImageProtocol::Kitty;
    }

    if term.as_deref().is_some_and(|value| value.contains("kitty")) {
        return TerminalImageProtocol::Kitty;
    }

    if term_program
        .as_deref()
        .is_some_and(|value| matches!(value, "ghostty"))
    {
        return TerminalImageProtocol::Kitty;
    }

    TerminalImageProtocol::None
}

fn detect_tmux_from_env<I, K, V>(env: I) -> bool
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    env.into_iter()
        .any(|(key, value)| key.as_ref() == "TMUX" && !value.as_ref().trim().is_empty())
}

fn build_kitty_clear_sequence() -> String {
    "\u{1b}_Ga=d,d=z,z=-1,q=2;\u{1b}\\".to_string()
}

fn wrap_for_tmux_passthrough(sequence: &str) -> String {
    let escaped = sequence.replace('\u{1b}', "\u{1b}\u{1b}");
    format!("\u{1b}Ptmux;{escaped}\u{1b}\\")
}

fn build_kitty_display_sequence(spec: &TerminalImageOverlaySpec, clear_previous: bool) -> String {
    let mut sequence = String::new();
    if clear_previous {
        sequence.push_str(&build_kitty_clear_sequence());
    }

    let payload = base64::engine::general_purpose::STANDARD.encode(spec.path.as_bytes());
    sequence.push_str(&format!("\u{1b}[{};{}H", spec.row + 1, spec.column + 1));
    sequence.push_str(&format!(
        "\u{1b}_Ga=T,f=100,t=f,c={},r={},C=1,z=-1,q=2;{}\u{1b}\\",
        spec.cols, spec.rows, payload
    ));
    sequence
}

#[cfg(test)]
pub(crate) fn set_active_protocol_for_tests(protocol: TerminalImageProtocol) {
    set_active_protocol(protocol);
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;

    use super::*;

    #[test]
    fn detect_protocol_prefers_explicit_override() {
        let env = [
            ("TAMUX_TUI_IMAGE_PROTOCOL", "kitty"),
            ("TMUX", "/tmp/tmux-1000/default,1,0"),
        ];

        assert_eq!(detect_protocol_from_env(env), TerminalImageProtocol::Kitty);
    }

    #[test]
    fn kitty_sequence_uses_file_transport_and_cell_geometry() {
        let spec = TerminalImageOverlaySpec {
            path: "/tmp/demo image.png".to_string(),
            column: 10,
            row: 6,
            cols: 40,
            rows: 12,
        };

        let sequence = build_kitty_display_sequence(&spec, true);
        let encoded_path = base64::engine::general_purpose::STANDARD.encode(spec.path.as_bytes());

        assert!(
            sequence.contains("\u{1b}[7;11H"),
            "expected cursor move before placement, got {sequence:?}"
        );
        assert!(
            sequence.contains("\u{1b}_Ga=d,d=z,z=-1,q=2;\u{1b}\\"),
            "expected visible overlay cleanup before redraw, got {sequence:?}"
        );
        assert!(
            sequence.contains("a=T,f=100,t=f,c=40,r=12,C=1,z=-1,q=2;"),
            "expected kitty placement metadata, got {sequence:?}"
        );
        assert!(
            sequence.contains(&encoded_path),
            "expected kitty placement to reference the image file path, got {sequence:?}"
        );
    }

    #[test]
    fn detect_protocol_keeps_kitty_inside_tmux_when_outer_terminal_supports_it() {
        let env = [
            ("KITTY_WINDOW_ID", "42"),
            ("TMUX", "/tmp/tmux-1000/default,1,0"),
        ];

        assert_eq!(detect_protocol_from_env(env), TerminalImageProtocol::Kitty);
    }

    #[test]
    fn tmux_passthrough_doubles_inner_escape_sequences() {
        let wrapped = wrap_for_tmux_passthrough("\u{1b}_Ga=d,d=z,z=-1;\u{1b}\\");

        assert!(
            wrapped.starts_with("\u{1b}Ptmux;"),
            "expected tmux passthrough prefix, got {wrapped:?}"
        );
        assert!(
            wrapped.contains("\u{1b}\u{1b}_G"),
            "expected inner ESC bytes to be doubled for tmux, got {wrapped:?}"
        );
        assert!(
            wrapped.ends_with("\u{1b}\\"),
            "expected tmux passthrough terminator, got {wrapped:?}"
        );
    }
}
