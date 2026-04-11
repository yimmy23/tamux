use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::theme::ThemeTokens;

struct LoadingCopy {
    headline: String,
    stages: [&'static str; 4],
    footer: String,
}

const CONCIERGE_STAGES: [&str; 4] = [
    "Reading the ember-thread",
    "Gathering sparks from recent memory",
    "Braiding omen, memory, and intent",
    "Threading the welcome from flame",
];

const THREAD_STAGES: [&str; 4] = [
    "Replaying recent turns",
    "Rehydrating tool state",
    "Refreshing runtime context",
    "Braiding thread summary and memory",
];

fn lower_centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let popup_width = width.min(area.width);
    let popup_height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(popup_width) / 2;
    let bottom_margin = 4u16.min(area.height.saturating_sub(popup_height));
    let y = area.y + area.height.saturating_sub(popup_height + bottom_margin);
    Rect::new(x, y, popup_width, popup_height)
}

fn morph_phase(tick: u64) -> usize {
    ((tick / 10) % 4) as usize
}

fn stage_label(stages: &[&'static str; 4], tick: u64) -> &'static str {
    stages[((tick / 28) % 4) as usize]
}

fn orbit_line(width: usize, tick: u64, reverse: bool) -> String {
    if width == 0 {
        return String::new();
    }

    let mut chars = vec!['.'; width];
    let step = ((tick / 3) as usize) % width;
    let primary = if reverse {
        width.saturating_sub(1).saturating_sub(step)
    } else {
        step
    };
    let secondary = if reverse {
        width
            .saturating_sub(1)
            .saturating_sub((step + width / 3) % width)
    } else {
        (step + width / 3) % width
    };
    chars[primary] = 'o';
    chars[secondary] = 'o';
    chars.into_iter().collect()
}

fn rarog_frame(tick: u64) -> &'static [&'static str] {
    match morph_phase(tick) {
        0 => &[
            "                                   ...................                  ",
            "                             ............###...#.........               ",
            "                           ...........################....              ",
            "                          .....########################......           ",
            "                          ...###########################.......         ",
            "                       .....#################.###########.......        ",
            "                     ..##########..########################......       ",
            "                  ##############.#...#####..################.....       ",
            "              #############.##......######.#################......      ",
            "             #######################.###..##################.......     ",
            "            ######......\\\\##.############################.......      ",
            "           ###.............########.###..#################........      ",
            "            ...............######..#####..###############.........      ",
            "          .................###############################.......       ",
            "        .............#.....#################################........    ",
            "       .............##.....##################################........   ",
            "      ..............##.....####################################.......  ",
            "       ............###....#####################################.......  ",
            "       ...........####...######################################.......  ",
            "      ...........#####...########################..############........ ",
            "   .............######....###############..#######....##########....    ",
        ],
        1 => &[
            "                                   ...................                  ",
            "                             ............###...#.........               ",
            "                           ...........################....              ",
            "                          .....########################......           ",
            "                          ...###########################.......         ",
            "                       .....#################.###########.......        ",
            "                     ..##########..########################......       ",
            "                  ##############.....#####..################.....       ",
            "              #############.##......######.#################......      ",
            "             #######################.###..##################.......     ",
            "            ######......\\\\##.############################.......      ",
            "           ###.............########.###..#################........      ",
            "            ...............######..#####..###############.........      ",
            "          .................###############################.......       ",
            "        .............#.....#################################........    ",
            "       .............##.....##################################........   ",
            "      .............###.....####################################.......  ",
            "       ...........####....#####################################.......  ",
            "       ..........#####...######################################.......  ",
            "      ..........######....########################..############....... ",
            "   .........##########....###############..#######....##########....    ",
        ],
        2 => &[
            "                                   ...................                  ",
            "                             ............###...#.........               ",
            "                           ...........################....              ",
            "                          .....########################......           ",
            "                          ...###########################.......         ",
            "                       .....#################.###########.......        ",
            "                     ..##########..########################......       ",
            "                  ##############.#...#####..################.....       ",
            "              #############.##......######.#################......      ",
            "             #######################.###..##################.......     ",
            "            ######......\\\\##.############################.......      ",
            "           ###.............########.###..#################........      ",
            "            ...............######..#####..###############.........      ",
            "          ...........#.....###############################.......       ",
            "        ..........####.....#################################........    ",
            "       ..........#####.....##################################........   ",
            "      ...........#####.....####################################.......  ",
            "       ........#######....#####################################.......  ",
            "       .......########...######################################.......  ",
            "      .......#########....########################..############....... ",
            "   .........##########....###############..#######....##########....    ",
        ],
        _ => &[
            "                                   ...................                  ",
            "                             ............###...#.........               ",
            "                           ...........################....              ",
            "                          .....########################......           ",
            "                          ...###########################.......         ",
            "                       .....#################.###########.......        ",
            "                     ..##########..########################......       ",
            "                  ##############.....#####..################.....       ",
            "              #############.##......######.#################......      ",
            "             #######################.###..##################.......     ",
            "            ######......\\\\##.############################.......      ",
            "           ###.............########.###..#################........      ",
            "            ...............######..#####..###############.........      ",
            "          .................###############################.......       ",
            "        ...................#################################........    ",
            "       ...................##################################........    ",
            "      ...................####################################.......    ",
            "       ...................#####################################.......  ",
            "       ...................######################################....... ",
            "      ...................########################..############.......  ",
            "   ......................###############..#######....##########....     ",
        ],
    }
}

fn portrait_frame(tick: u64) -> &'static [&'static str] {
    match morph_phase(tick) {
        0 => &[
            "......      .-----.      ......",
            "....      .'  -  '.      ....",
            "...      /   o o   \\      ...",
            "..      |     ^     |      ..",
            "..      |    ---    |      ..",
            "...      \\   ===   /      ...",
            "....      '.___.'      ....",
        ],
        1 => &[
            "......      .--=--.      ......",
            "....      .'  -  '.      ....",
            "...      /   o #   \\      ...",
            "..      |    /_\\    |      ..",
            "..      |   <-=>    |      ..",
            "...      \\   ===   /      ...",
            "....      '._=_.'      ....",
        ],
        2 => &[
            "......      .-===-.      ......",
            "....      .' _=_ '.      ....",
            "...      /  [0 0]  \\      ...",
            "..      |    /_\\    |      ..",
            "..      |   [###]   |      ..",
            "...      \\  _===_  /      ...",
            "....      '._____.'      ....",
        ],
        _ => &[
            "......      .-#=#-.      ......",
            "....      .'_\\^/_'.      ....",
            "...      /  o>#<0  \\      ...",
            "..      |    /#\\    |      ..",
            "..      |   [_#_]   |      ..",
            "...      \\  =#=#=  /      ...",
            "....      '.__#.'      ....",
        ],
    }
}

fn glyph_style(ch: char, theme: &ThemeTokens) -> Style {
    if ch == '.' {
        theme.fg_dim
    } else if ch == ' ' {
        Style::default()
    } else {
        theme.fg_active.add_modifier(Modifier::BOLD)
    }
}

fn styled_glyph_line(text: &str, theme: &ThemeTokens) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current_style = None;
    let mut current_text = String::new();

    for ch in text.chars() {
        let style = glyph_style(ch, theme);
        if current_style != Some(style) && !current_text.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut current_text),
                current_style.expect("current style should exist"),
            ));
        }
        current_style = Some(style);
        current_text.push(ch);
    }

    if !current_text.is_empty() {
        spans.push(Span::styled(
            current_text,
            current_style.expect("final style should exist"),
        ));
    }

    Line::from(spans)
}

pub fn render(frame: &mut Frame, area: Rect, theme: &ThemeTokens, tick: u64) {
    render_with_copy(frame, area, theme, tick, concierge_copy());
}

pub fn render_thread(
    frame: &mut Frame,
    area: Rect,
    theme: &ThemeTokens,
    tick: u64,
    thread_title: Option<&str>,
) {
    render_with_copy(frame, area, theme, tick, thread_copy(thread_title));
}

fn concierge_copy() -> LoadingCopy {
    LoadingCopy {
        headline: "Rarog is threading a welcome".to_string(),
        stages: CONCIERGE_STAGES,
        footer: "ember sync: human intuition <-> fire-memory".to_string(),
    }
}

fn thread_copy(thread_title: Option<&str>) -> LoadingCopy {
    let title = normalized_thread_title(thread_title);
    LoadingCopy {
        headline: format!("Loading thread: {title}"),
        stages: THREAD_STAGES,
        footer: format!("thread summary: {title}"),
    }
}

fn normalized_thread_title(thread_title: Option<&str>) -> String {
    let trimmed = thread_title.unwrap_or_default().trim();
    if trimmed.is_empty() {
        "current thread".to_string()
    } else {
        trimmed.to_string()
    }
}

fn render_with_copy(
    frame: &mut Frame,
    area: Rect,
    theme: &ThemeTokens,
    tick: u64,
    copy: LoadingCopy,
) {
    frame.render_widget(Clear, area);

    let inner = lower_centered_rect(80, 26, area);
    let orbit_width = inner.width.saturating_sub(6) as usize;
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        copy.headline,
        theme.fg_active.add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        stage_label(&copy.stages, tick),
        theme.fg_dim,
    )));
    lines.push(styled_glyph_line(
        &orbit_line(orbit_width, tick, false),
        theme,
    ));
    for row in rarog_frame(tick) {
        lines.push(Line::from(Span::styled(
            *row,
            theme.fg_dim.add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(styled_glyph_line(
        &orbit_line(orbit_width, tick + 5, true),
        theme,
    ));
    lines.push(Line::from(Span::styled(copy.footer, theme.fg_dim)));

    frame.render_widget(Paragraph::new(lines).centered(), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_label_cycles_through_loading_states() {
        assert_eq!(
            stage_label(&CONCIERGE_STAGES, 0),
            "Reading the ember-thread"
        );
        assert_eq!(
            stage_label(&CONCIERGE_STAGES, 28),
            "Gathering sparks from recent memory"
        );
        assert_eq!(
            stage_label(&CONCIERGE_STAGES, 56),
            "Braiding omen, memory, and intent"
        );
        assert_eq!(
            stage_label(&CONCIERGE_STAGES, 84),
            "Threading the welcome from flame"
        );
    }

    #[test]
    fn thread_copy_uses_thread_specific_labels() {
        let copy = thread_copy(Some("Release planning"));

        assert_eq!(copy.headline, "Loading thread: Release planning");
        assert_eq!(copy.stages[0], "Replaying recent turns");
        assert_eq!(copy.footer, "thread summary: Release planning");
    }

    #[test]
    fn orbit_line_advances_markers_over_time() {
        assert_ne!(orbit_line(24, 0, false), orbit_line(24, 6, false));
        assert_ne!(orbit_line(24, 0, true), orbit_line(24, 6, true));
    }
}
