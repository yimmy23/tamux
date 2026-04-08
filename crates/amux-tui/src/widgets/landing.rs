use ratatui::prelude::*;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::ThemeTokens;

// const SVAROG_FRESCO_STATIC: &[&str] = &[
//     ":::::       .-#===#-.       :::::",
//     "::::       /  |_|_|  \\       ::::",
//     ":::       |  ( o o )  |       :::",
//     "::        |    \\#/    |        ::",
//     "::        |     |     |        ::",
//     "::        |    /#\\    |        ::",
//     ":::        \\   ###   /        :::",
//     "::::        `-.___.-'        ::::",
// ];

const SVAROG_FRESCO_STATIC: &[&str] = &[
    "                                          .......                       ",
    "                 ...                     ....#....           ..         ",
    "               ....                  .......##.....          ..         ",
    "           ...  .. ..              ........#.#.......        ...        ",
    "           ...........            ........#..#........       ...        ",
    "    ...      .........     .      .##.##......##..##...  ##....         ",
    "   ....  ..............   .#...##.###.#...........###..#....##          ",
    "   ......########...........#####.###....####....############           ",
    "     .#############...........#########.........###########             ",
    "  ...####.......####............##########...###########.               ",
    "  ####.#.........####..........####.....##....##....###...              ",
    "####.##..........####.........#####.........#......########.            ",
    "#####....##########...........########......#.....#######...#..         ",
    "######.########..............#######.#....###....##..#####..##..        ",
    " ############..............##########..#########.....######.#####       ",
    "  .##......####............##.#####..######.######...#####.##.........  ",
    " ...........###........#####.######.####........#.#################.... ",
    "  ...........###......###############.....###.....#################..  ",
    " ..    .......###......##############.............##############........",
    " ..............###....##################........##############...##.....",
    "................###.....###..##############.###############..##......   ",
];

const RAROG_FRESCO_STATIC: &[&str] = &[
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
];

const WELES_FRESCO_STATIC: &[&str] = &[
    "                                                                        ",
    "                    ..          ...                          ..         ",
    "                    ..      .......#         .             ......       ",
    "                    #.... .........##     .....##..#.    ..####...      ",
    "                    .......###.....#....##...##...##.. ..## # #...      ",
    "                      #########.......######............## ## ##...     ",
    "                         ########..#########.............# ### #....    ",
    "                       .#.##..##...##...######...........#####.....     ",
    "                      ..#####....#.....#####.###.........####.....      ",
    "                      ..###....#####...#########..........###.....      ",
    "                    .##..##...#.###.##..###########.......###.....      ",
    "                 ..#.####.##.....#......################..###.....      ",
    "              #####.#########...###....#################..###.....      ",
    "              ....##..############.##########################.....      ",
    "              .....#.#..####..##########.###.###########.###....        ",
    "                 ..  #########.........####..#..#######...##.....       ",
    "                             #.##...##.#         .####....##.....       ",
    "                                ..###.             ###....##.....       ",
    "                                ...  ..            .##....#......       ",
    "                                                   ..............       ",
    "                                                                        ",
];

fn fresco_for_agent(agent_label: &str) -> &'static [&'static str] {
    match agent_label {
        "Rarog" => RAROG_FRESCO_STATIC,
        "Weles" => WELES_FRESCO_STATIC,
        _ => SVAROG_FRESCO_STATIC,
    }
}

fn copy_for_agent(agent_label: &str) -> (&'static str, &'static str) {
    match agent_label {
        "Rarog" => ("Cinders turn.", "Rarog watches the seam."),
        "Weles" => ("The ward is drawn.", "Weles weighs the path."),
        _ => ("Fire is lit.", "Svarog tends the forge."),
    }
}

fn content_width(line: &Line<'_>) -> u16 {
    let width = line
        .spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    width.min(u16::MAX as usize) as u16
}

fn centered_content_rect(area: Rect, lines: &[Line<'_>]) -> Rect {
    let content_width = lines.iter().map(content_width).max().unwrap_or(1);
    let content_height = lines.len().min(area.height as usize) as u16;
    let width = content_width.min(area.width).max(1);
    let height = content_height.min(area.height).max(1);
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height) / 2);
    Rect::new(x, y, width, height)
}

fn render_line_clipped(frame: &mut Frame, area: Rect, row: u16, line: &Line<'_>) {
    if row >= area.height {
        return;
    }

    let y = area.y.saturating_add(row);
    let mut x = area.x;
    let max_x = area.x.saturating_add(area.width);
    let line_style = line.style;

    for span in &line.spans {
        let style = line_style.patch(span.style);
        for ch in span.content.chars() {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
            if width == 0 {
                continue;
            }
            if x >= max_x {
                return;
            }

            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_symbol(&ch.to_string());
                cell.set_style(style);
                cell.skip = false;
            }

            if width > 1 {
                for offset in 1..width {
                    let continuation_x = x.saturating_add(offset);
                    if continuation_x >= max_x {
                        break;
                    }
                    if let Some(cell) = frame.buffer_mut().cell_mut((continuation_x, y)) {
                        cell.reset();
                        cell.set_style(style);
                        cell.skip = true;
                    }
                }
            }

            x = x.saturating_add(width);
        }
    }
}

fn centered_line_rect(content_area: Rect, row: u16, line: &Line<'_>) -> Rect {
    let width = content_width(line).min(content_area.width).max(1);
    let x = content_area
        .x
        .saturating_add(content_area.width.saturating_sub(width) / 2);
    Rect::new(x, content_area.y.saturating_add(row), width, 1)
}

pub fn render(frame: &mut Frame, area: Rect, theme: &ThemeTokens, agent_label: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut lines: Vec<Line<'static>> = Vec::new();

    let (lead_copy, agent_copy) = copy_for_agent(agent_label);

    for row in fresco_for_agent(agent_label) {
        lines.push(Line::from(Span::styled(
            *row,
            theme.fg_dim.add_modifier(Modifier::BOLD),
        )));
    }

    // lines.push(Line::from(vec![
    //     Span::styled("\u{2591}", Style::default().fg(Color::Indexed(24))),
    //     Span::styled("\u{2592}", Style::default().fg(Color::Indexed(31))),
    //     Span::styled("\u{2593}", Style::default().fg(Color::Indexed(38))),
    //     Span::styled("\u{2588}", Style::default().fg(Color::Indexed(75))),
    //     Span::styled(" T A M U X ", theme.accent_primary),
    //     Span::styled("\u{2588}", Style::default().fg(Color::Indexed(75))),
    //     Span::styled("\u{2593}", Style::default().fg(Color::Indexed(38))),
    //     Span::styled("\u{2592}", Style::default().fg(Color::Indexed(31))),
    //     Span::styled("\u{2591}", Style::default().fg(Color::Indexed(24))),
    // ]));
    // lines.push(Line::from(Span::styled(
    //     "think \u{00b7} plan \u{00b7} ship",
    //     theme.fg_dim,
    // )));
    lines.push(Line::raw(""));
    // lines.push(Line::from(Span::styled(
    //     "Clean thread. Svarog is here. Type to begin.",
    //     theme.fg_dim,
    // )));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(lead_copy, theme.fg_dim),
        Span::raw("  "),
        Span::styled(agent_copy, theme.accent_secondary),
        Span::raw("  "),
        Span::styled("Type to begin.", theme.fg_dim),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Ctrl+P", theme.accent_primary),
        Span::styled(" command palette  ", theme.fg_dim),
        Span::styled("Ctrl+T", theme.accent_primary),
        Span::styled(" threads", theme.fg_dim),
    ]));

    let content_area = centered_content_rect(area, &lines);
    for (row, line) in lines.iter().enumerate() {
        if row >= content_area.height as usize {
            break;
        }
        let line_area = centered_line_rect(content_area, row as u16, line);
        render_line_clipped(frame, line_area, 0, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_rows_for(agent_label: &str, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        terminal
            .draw(|frame| render(frame, frame.area(), &ThemeTokens::default(), agent_label))
            .expect("landing render should succeed");

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect()
    }

    fn render_plain(agent_label: &str, width: u16, height: u16) -> String {
        render_rows_for(agent_label, width, height).join("\n")
    }

    #[test]
    fn centered_content_rect_stays_inside_target_area() {
        let area = Rect::new(0, 0, 80, 24);
        let lines = vec![
            Line::from("short"),
            Line::from("a much wider centered line"),
        ];

        let rect = centered_content_rect(area, &lines);

        assert!(rect.x >= area.x);
        assert!(rect.y >= area.y);
        assert!(rect.x.saturating_add(rect.width) <= area.x.saturating_add(area.width));
        assert!(rect.y.saturating_add(rect.height) <= area.y.saturating_add(area.height));
    }

    #[test]
    fn content_width_counts_styled_spans_without_overflow() {
        let line = Line::from(vec![
            Span::styled("abc", Style::default()),
            Span::styled("def", Style::default()),
        ]);

        assert_eq!(content_width(&line), 6);
    }

    #[test]
    fn centered_line_rect_centers_shorter_lines_within_content_area() {
        let content_area = Rect::new(10, 5, 30, 8);
        let line = Line::from("short");

        let rect = centered_line_rect(content_area, 2, &line);

        assert_eq!(rect.y, 7);
        assert!(rect.x > content_area.x);
        assert_eq!(rect.width, 5);
    }

    #[test]
    fn landing_render_balances_vertical_space() {
        let rows = render_rows_for("Svarog", 80, 24);
        let visible_rows: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter_map(|(idx, row)| (!row.trim().is_empty()).then_some(idx))
            .collect();

        let first = *visible_rows
            .first()
            .expect("landing should render visible rows");
        let last = *visible_rows
            .last()
            .expect("landing should render visible rows");
        let top_padding = first;
        let bottom_padding = rows.len().saturating_sub(last + 1);

        assert!(
            top_padding.abs_diff(bottom_padding) <= 1,
            "landing should be vertically centered, got top={top_padding} bottom={bottom_padding}"
        );
    }

    #[test]
    fn landing_copy_is_unique_per_agent() {
        let svarog = render_plain("Svarog", 80, 24);
        let rarog = render_plain("Rarog", 80, 24);
        let weles = render_plain("Weles", 80, 24);

        assert!(
            svarog.contains("Fire is lit."),
            "expected Svarog copy, got: {svarog}"
        );
        assert!(
            rarog.contains("Cinders turn."),
            "expected Rarog copy, got: {rarog}"
        );
        assert!(
            weles.contains("The ward is drawn."),
            "expected Weles copy, got: {weles}"
        );

        assert!(
            !rarog.contains("Fire is lit."),
            "Rarog should not reuse Svarog copy: {rarog}"
        );
        assert!(
            !weles.contains("Fire is lit."),
            "Weles should not reuse Svarog copy: {weles}"
        );
        assert!(
            !weles.contains("Cinders turn."),
            "Weles should not reuse Rarog copy: {weles}"
        );
    }

    #[test]
    fn landing_ascii_is_unique_per_agent() {
        let rarog = render_plain("Rarog", 80, 24);
        let weles = render_plain("Weles", 80, 24);

        assert!(
            rarog.contains(".....#################.###########......."),
            "expected dedicated Rarog fresco signature, got: {rarog}"
        );
        assert!(
            weles.contains("#########.......######............## ## ##..."),
            "expected dedicated Weles fresco signature, got: {weles}"
        );
    }

    #[test]
    fn landing_render_centers_each_line_individually() {
        let rows = render_rows_for("Svarog", 80, 24);
        let art_row = rows
            .iter()
            .find(|row| row.contains("......."))
            .expect("art row should be rendered");
        let body_row = rows
            .iter()
            .find(|row| row.contains("Fire is lit."))
            .expect("body row should be rendered");

        assert!(
            art_row.chars().any(|ch| ch != ' '),
            "art row should have visible content"
        );
        let body_start = body_row
            .chars()
            .position(|ch| ch != ' ')
            .expect("body row should have visible content");

        assert!(
            body_start > 0,
            "shorter landing rows should be centered away from the left edge"
        );
    }
}
