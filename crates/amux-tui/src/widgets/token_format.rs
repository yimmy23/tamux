const TOKEN_UNITS: [&str; 6] = ["", "k", "M", "B", "T", "P"];

pub(crate) fn format_token_count(tokens: u64) -> String {
    if tokens < 1_000 {
        return format!("{} tok", tokens);
    }

    let mut value = tokens as f64;
    let mut unit_index = 0usize;

    while value >= 999.95 && unit_index + 1 < TOKEN_UNITS.len() {
        value /= 1_000.0;
        unit_index += 1;
    }

    format!("{value:.1}{} tok", TOKEN_UNITS[unit_index])
}

#[cfg(test)]
mod tests {
    use super::format_token_count;

    #[test]
    fn format_token_count_formats_zero() {
        assert_eq!(format_token_count(0), "0 tok");
    }

    #[test]
    fn format_token_count_formats_small_values() {
        assert_eq!(format_token_count(500), "500 tok");
    }

    #[test]
    fn format_token_count_formats_thousands() {
        assert_eq!(format_token_count(1_500), "1.5k tok");
    }

    #[test]
    fn format_token_count_switches_to_millions() {
        assert_eq!(format_token_count(1_500_000), "1.5M tok");
    }

    #[test]
    fn format_token_count_switches_to_billions() {
        assert_eq!(format_token_count(1_500_000_000), "1.5B tok");
    }

    #[test]
    fn format_token_count_promotes_on_rounding_boundary() {
        assert_eq!(format_token_count(999_950_000), "1.0B tok");
    }
}