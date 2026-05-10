use crate::state::task;

pub(crate) fn convert_work_context(c: crate::wire::ThreadWorkContext) -> task::ThreadWorkContext {
    task::ThreadWorkContext {
        thread_id: c.thread_id,
        entries: c
            .entries
            .into_iter()
            .map(|entry| task::WorkContextEntry {
                path: entry.path,
                previous_path: entry.previous_path,
                kind: entry.kind.map(|kind| match kind {
                    crate::wire::WorkContextEntryKind::RepoChange => {
                        task::WorkContextEntryKind::RepoChange
                    }
                    crate::wire::WorkContextEntryKind::Artifact => {
                        task::WorkContextEntryKind::Artifact
                    }
                    crate::wire::WorkContextEntryKind::GeneratedSkill => {
                        task::WorkContextEntryKind::GeneratedSkill
                    }
                }),
                source: entry.source,
                change_kind: entry.change_kind,
                repo_root: entry.repo_root,
                goal_run_id: entry.goal_run_id,
                step_index: entry.step_index,
                session_id: entry.session_id,
                is_text: entry.is_text,
                updated_at: entry.updated_at,
            })
            .collect(),
    }
}

pub(crate) fn convert_heartbeat(h: crate::wire::HeartbeatItem) -> task::HeartbeatItem {
    task::HeartbeatItem {
        id: h.id,
        label: h.label,
        outcome: h.last_result.map(|r| match r {
            crate::wire::HeartbeatOutcome::Ok => task::HeartbeatOutcome::Ok,
            crate::wire::HeartbeatOutcome::Alert => task::HeartbeatOutcome::Warn,
            crate::wire::HeartbeatOutcome::Error => task::HeartbeatOutcome::Error,
        }),
        message: h.last_message,
        timestamp: 0,
    }
}

#[cfg(test)]
static LAST_COPIED_TEXT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    static TEST_CLIPBOARD_OWNER_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn reset_last_copied_text() {
    *LAST_COPIED_TEXT
        .lock()
        .expect("clipboard test mutex poisoned") = None;
    TEST_CLIPBOARD_OWNER_HELD.with(|held| held.set(false));
}

#[cfg(test)]
pub(crate) fn last_copied_text() -> Option<String> {
    LAST_COPIED_TEXT
        .lock()
        .expect("clipboard test mutex poisoned")
        .clone()
}

#[cfg(test)]
pub(crate) fn test_clipboard_owner_held() -> bool {
    TEST_CLIPBOARD_OWNER_HELD.with(std::cell::Cell::get)
}

pub(crate) fn copy_to_clipboard(text: &str) {
    #[cfg(test)]
    {
        *LAST_COPIED_TEXT
            .lock()
            .expect("clipboard test mutex poisoned") = Some(text.to_string());
        TEST_CLIPBOARD_OWNER_HELD.with(|held| held.set(true));
        return;
    }

    #[cfg(not(test))]
    {
        use super::convert_thread_to_convert_todo_with_fallback_step::SYSTEM_CLIPBOARD;
        use base64::Engine;

        let copied = SYSTEM_CLIPBOARD.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = arboard::Clipboard::new().ok();
            }

            slot.as_mut()
                .map(|clipboard| clipboard.set_text(text.to_string()).is_ok())
                .unwrap_or(false)
        });

        if !copied {
            let encoded = base64::engine::general_purpose::STANDARD.encode(text);
            print!("\x1b]52;c;{}\x07", encoded);
        }
    }
}
