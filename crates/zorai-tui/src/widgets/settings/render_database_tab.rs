use crate::state::config::ConfigState;
use crate::state::settings::SettingsState;
use crate::theme::ThemeTokens;
use ratatui::text::{Line, Span};
pub(crate) fn render_database_tab<'a>(
    settings: &SettingsState,
    config: &ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let is_selected = settings.field_cursor() == 0;
    let marker = if is_selected { "> " } else { "  " };
    let action_style = if is_selected {
        theme.fg_active
    } else {
        theme.fg_dim
    };
    let backend = if config.db_backend.is_empty() {
        "local (sqlite)".to_string()
    } else {
        config.db_backend.clone()
    };
    let sync_url = if config.db_sync_url.is_empty() {
        "(unset)".to_string()
    } else {
        config.db_sync_url.clone()
    };
    vec![
        Line::from(Span::styled("Database backend", theme.fg_active)),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Backend:      ", theme.fg_dim),
            Span::raw(backend),
        ]),
        Line::from(vec![
            Span::styled("Sync URL:     ", theme.fg_dim),
            Span::raw(sync_url),
        ]),
        Line::from(vec![
            Span::styled("Auth token:   ", theme.fg_dim),
            Span::raw(if config.db_has_token { "set" } else { "unset" }),
        ]),
        Line::from(vec![
            Span::styled("Seeded remote:", theme.fg_dim),
            Span::raw(if config.db_seeded_at.is_some() {
                " yes"
            } else {
                " no"
            }),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(marker, action_style),
            Span::styled("Sync now", action_style),
            Span::styled("  (sync the local replica with the remote server)", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(Span::styled("Configure in config.json:", theme.fg_active)),
        Line::from(vec![
            Span::styled("  db_backend           ", theme.fg_dim),
            Span::raw("local (default) | local-libsql | remote-replica"),
        ]),
        Line::from(vec![
            Span::styled("  db_sync_url          ", theme.fg_dim),
            Span::raw("libSQL/Turso server URL (remote-replica)"),
        ]),
        Line::from(vec![
            Span::styled("  db_sync_interval_secs", theme.fg_dim),
            Span::raw("  background sync cadence (default 60)"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Auth token:  ", theme.fg_dim),
            Span::raw("ZORAI_DB_AUTH_TOKEN env or ~/.config/zorai/db_auth_token"),
        ]),
        Line::raw(""),
        Line::from(Span::styled("CLI:", theme.fg_active)),
        Line::from(vec![
            Span::styled("  zorai-daemon db push  ", theme.fg_dim),
            Span::raw("seed local data up to the remote server"),
        ]),
    ]
}
