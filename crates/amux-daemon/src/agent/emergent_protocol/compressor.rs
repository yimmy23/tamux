pub(crate) fn compress_pattern_key(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn stable_protocol_token(normalized_pattern: &str, thread_id: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!("{thread_id}:{normalized_pattern}").bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("@proto_{:08x}", (hash & 0xffff_ffff) as u32)
}
