pub fn format_duration_ms(duration_ms: u64) -> String {
    let seconds = (duration_ms / 1000).max(1);
    if seconds < 120 {
        format!("{seconds}s")
    } else {
        format!("{}m", (seconds + 30) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::format_duration_ms;

    #[test]
    fn zero_ms_clamps_to_one_second() {
        assert_eq!(format_duration_ms(0), "1s");
    }

    #[test]
    fn sub_second_clamps_to_one_second() {
        assert_eq!(format_duration_ms(999), "1s");
    }

    #[test]
    fn one_second_exact() {
        assert_eq!(format_duration_ms(1_000), "1s");
    }

    #[test]
    fn sixty_seconds_uses_seconds_format() {
        assert_eq!(format_duration_ms(60_000), "60s");
    }

    #[test]
    fn just_under_two_minutes_uses_seconds_format() {
        assert_eq!(format_duration_ms(119_000), "119s");
    }

    #[test]
    fn two_minutes_boundary_switches_to_minutes() {
        assert_eq!(format_duration_ms(120_000), "2m");
    }

    #[test]
    fn rounds_down_below_thirty_seconds_into_next_minute() {
        // 149s = 2m29s → rounds to 2m
        assert_eq!(format_duration_ms(149_000), "2m");
    }

    #[test]
    fn rounds_up_at_thirty_seconds_into_next_minute() {
        // 150s = 2m30s → rounds to 3m
        assert_eq!(format_duration_ms(150_000), "3m");
    }

    #[test]
    fn one_hour_renders_as_sixty_minutes() {
        assert_eq!(format_duration_ms(3_600_000), "60m");
    }
}
