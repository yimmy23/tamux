use amux_protocol::{OscNotificationPayload, OscSource};

/// Parse OSC notification sequences from terminal output bytes.
/// Returns a list of parsed notifications and the cleaned output.
pub fn parse_osc_notifications(data: &[u8]) -> (Vec<OscNotificationPayload>, Vec<u8>) {
    let mut notifications = Vec::new();
    let mut cleaned = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        // OSC starts with ESC ] (0x1b 0x5d) or 0x9d (C1 control)
        if (i + 1 < data.len() && data[i] == 0x1b && data[i + 1] == 0x5d) || data[i] == 0x9d {
            let osc_start = if data[i] == 0x9d { i + 1 } else { i + 2 };

            // Find the end: ST = ESC \ (0x1b 0x5c) or BEL (0x07)
            let mut end = osc_start;
            let mut st_len = 0;
            while end < data.len() {
                if data[end] == 0x07 {
                    st_len = 1;
                    break;
                }
                if end + 1 < data.len() && data[end] == 0x1b && data[end + 1] == 0x5c {
                    st_len = 2;
                    break;
                }
                end += 1;
            }

            if st_len > 0 {
                let payload = &data[osc_start..end];
                if let Ok(text) = std::str::from_utf8(payload) {
                    if let Some(notif) = try_parse_osc(text) {
                        notifications.push(notif);
                        i = end + st_len;
                        continue;
                    }
                }
            }

            // Not a recognized notification OSC — pass through
            cleaned.push(data[i]);
            i += 1;
        } else {
            cleaned.push(data[i]);
            i += 1;
        }
    }

    (notifications, cleaned)
}

fn try_parse_osc(text: &str) -> Option<OscNotificationPayload> {
    // OSC 9 ; <message> — iTerm2 Growl notification
    if let Some(rest) = text.strip_prefix("9;") {
        return Some(OscNotificationPayload {
            source: OscSource::Osc9,
            title: rest.to_string(),
            body: String::new(),
            subtitle: None,
            icon: None,
            progress: None,
        });
    }

    // OSC 777 ; notify ; <title> ; <body> — rxvt-unicode
    if let Some(rest) = text.strip_prefix("777;notify;") {
        let mut parts = rest.splitn(2, ';');
        let title = parts.next().unwrap_or("").to_string();
        let body = parts.next().unwrap_or("").to_string();
        return Some(OscNotificationPayload {
            source: OscSource::Osc777,
            title,
            body,
            subtitle: None,
            icon: None,
            progress: None,
        });
    }

    // OSC 99 — Kitty notification protocol (key=value pairs separated by ;)
    // Format: 99;i=<id>:d=0:p=body;<text>
    // or simple: 99;<text>
    if let Some(rest) = text.strip_prefix("99;") {
        // Simple form: just text
        let mut title = String::new();
        let mut body = rest.to_string();

        // Try to parse key=value pairs
        if rest.contains(':') || rest.contains(';') {
            let parts: Vec<&str> = rest.splitn(2, ';').collect();
            if parts.len() == 2 {
                // First part has key=value metadata, second is content
                body = parts[1].to_string();
                for kv in parts[0].split(':') {
                    if let Some((_k, _v)) = kv.split_once('=') {
                        // Could extract i=, d=, p= etc.
                    }
                }
            }
        }

        if title.is_empty() {
            title = "Notification".to_string();
        }

        return Some(OscNotificationPayload {
            source: OscSource::Osc99,
            title,
            body,
            subtitle: None,
            icon: None,
            progress: None,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_osc9() {
        // ESC ] 9 ; Hello BEL
        let data = b"\x1b]9;Hello World\x07rest";
        let (notifs, cleaned) = parse_osc_notifications(data);
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].title, "Hello World");
        assert_eq!(cleaned, b"rest");
    }

    #[test]
    fn parse_osc777() {
        let data = b"\x1b]777;notify;Build;Complete\x07";
        let (notifs, _) = parse_osc_notifications(data);
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].title, "Build");
        assert_eq!(notifs[0].body, "Complete");
    }

    #[test]
    fn non_notification_osc_passes_through() {
        // OSC 0 (set title) should pass through
        let data = b"\x1b]0;my title\x07hello";
        let (notifs, cleaned) = parse_osc_notifications(data);
        assert_eq!(notifs.len(), 0);
        // The OSC 0 is passed through to cleaned
        assert!(cleaned.len() > 0);
    }
}
