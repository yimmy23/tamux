use ratatui::prelude::*;

use super::to_core_color_to_render_status_bar::StatusBarHitTarget;

pub fn status_bar_hit_test(
    area: Rect,
    connected: bool,
    has_error: bool,
    voice_recording: bool,
    voice_playing: bool,
    queued_count: usize,
    position: Position,
) -> Option<StatusBarHitTarget> {
    if queued_count == 0 || position.y != area.y {
        return None;
    }

    let mut x = area.x;
    x = x.saturating_add(1);
    x = x.saturating_add(1);
    x = x.saturating_add(" daemon".chars().count() as u16);

    if has_error {
        x = x.saturating_add(2);
        x = x.saturating_add(1);
        x = x.saturating_add(" error".chars().count() as u16);
    }

    if voice_recording {
        x = x.saturating_add(2);
        x = x.saturating_add(1);
        x = x.saturating_add(" REC".chars().count() as u16);
    }

    if voice_playing {
        x = x.saturating_add(2);
        x = x.saturating_add(unicode_width::UnicodeWidthStr::width("🔊 PLAYING") as u16);
    }

    let _ = connected;
    x = x.saturating_add(2);
    let queued_width = 1 + format!(" queued({queued_count})").chars().count() as u16;
    if position.x >= x && position.x < x.saturating_add(queued_width) {
        return Some(StatusBarHitTarget::QueuedPrompts);
    }

    None
}
