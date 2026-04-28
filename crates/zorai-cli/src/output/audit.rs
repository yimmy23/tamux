/// Parse a human duration suffix ("1h", "24h", "7d") into an epoch-millis
/// timestamp representing `now - duration`.
pub(crate) fn parse_duration_ago(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num_str, multiplier_ms) = if let Some(num) = s.strip_suffix('d') {
        (num, 86_400_000u64)
    } else if let Some(num) = s.strip_suffix('h') {
        (num, 3_600_000u64)
    } else if let Some(num) = s.strip_suffix('m') {
        (num, 60_000u64)
    } else {
        return None;
    };
    let num: u64 = num_str.parse().ok()?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    Some(now_ms.saturating_sub(num * multiplier_ms))
}

/// Format an epoch-millis timestamp as `YYYY-MM-DD HH:MM` (UTC).
pub(crate) fn format_timestamp(epoch_ms: i64) -> String {
    let secs = epoch_ms / 1000;
    let duration = std::time::Duration::from_secs(secs.unsigned_abs());
    let system_time = if secs >= 0 {
        std::time::UNIX_EPOCH + duration
    } else {
        std::time::UNIX_EPOCH - duration
    };
    let formatted = humantime::format_rfc3339_seconds(system_time).to_string();
    if formatted.len() >= 16 {
        let date_part = &formatted[..10];
        let time_part = &formatted[11..16];
        format!("{} {}", date_part, time_part)
    } else {
        formatted
    }
}

/// Print a single audit row in the CLI table format.
pub(crate) fn print_audit_row(entry: &zorai_protocol::AuditEntryPublic) {
    let ts = format_timestamp(entry.timestamp);
    let confidence_tag = match (&entry.confidence_band, entry.confidence) {
        (Some(band), Some(pct)) if band != "confident" => {
            format!(" [{} {}%]", band, (pct * 100.0) as u32)
        }
        _ => String::new(),
    };
    println!(
        "{} [{}] [{}]{} {}",
        entry.id, ts, entry.action_type, confidence_tag, entry.summary
    );
}

/// Print full detail for a single audit entry.
pub(crate) fn print_audit_detail(entry: &zorai_protocol::AuditEntryPublic) {
    let ts = format_timestamp(entry.timestamp);
    println!("ID:          {}", entry.id);
    println!("Time:        {}", ts);
    println!("Type:        {}", entry.action_type);
    println!("Summary:     {}", entry.summary);
    println!(
        "Explanation: {}",
        entry.explanation.as_deref().unwrap_or("N/A")
    );
    match (&entry.confidence_band, entry.confidence) {
        (Some(band), Some(pct)) => {
            println!("Confidence:  {} ({}%)", band, (pct * 100.0) as u32);
        }
        _ => {
            println!("Confidence:  N/A");
        }
    }
    println!(
        "Trace:       {}",
        entry.causal_trace_id.as_deref().unwrap_or("N/A")
    );
    println!(
        "Thread:      {}",
        entry.thread_id.as_deref().unwrap_or("N/A")
    );
    if let Some(goal) = &entry.goal_run_id {
        println!("Goal Run:    {}", goal);
    }
    if let Some(task) = &entry.task_id {
        println!("Task:        {}", task);
    }
}
