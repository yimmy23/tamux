use crate::output::audit::{format_timestamp, parse_duration_ago};

#[test]
fn parse_duration_ago_rejects_unknown_suffixes() {
    assert_eq!(parse_duration_ago("15"), None);
    assert_eq!(parse_duration_ago("2w"), None);
}

#[test]
fn parse_duration_ago_accepts_minutes_hours_and_days() {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let minute = parse_duration_ago("1m").expect("minute duration");
    let hour = parse_duration_ago("1h").expect("hour duration");
    let day = parse_duration_ago("1d").expect("day duration");

    assert!(minute <= now_ms && minute >= now_ms.saturating_sub(61_000));
    assert!(hour <= now_ms && hour >= now_ms.saturating_sub(3_601_000));
    assert!(day <= now_ms && day >= now_ms.saturating_sub(86_401_000));
}

#[test]
fn format_timestamp_uses_expected_utc_layout() {
    assert_eq!(format_timestamp(0), "1970-01-01 00:00");
    assert_eq!(format_timestamp(1_710_000_000_000), "2024-03-09 16:00");
}
