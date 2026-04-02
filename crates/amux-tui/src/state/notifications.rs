use amux_protocol::{InboxNotification, InboxNotificationAction};

const MAX_NOTIFICATIONS: usize = 500;

#[derive(Debug, Clone)]
pub enum NotificationsAction {
    Replace(Vec<InboxNotification>),
    Upsert(InboxNotification),
    Select(usize),
    Navigate(i32),
    ToggleExpand(String),
}

pub struct NotificationsState {
    items: Vec<InboxNotification>,
    selected_index: usize,
    expanded_id: Option<String>,
}

impl NotificationsState {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected_index: 0,
            expanded_id: None,
        }
    }

    pub fn reduce(&mut self, action: NotificationsAction) {
        match action {
            NotificationsAction::Replace(items) => {
                self.items = sort_notifications(items);
                self.items.truncate(MAX_NOTIFICATIONS);
                self.selected_index = self
                    .selected_index
                    .min(self.active_items().len().saturating_sub(1));
                if self
                    .expanded_id
                    .as_deref()
                    .is_some_and(|id| self.item_by_id(id).is_none() || self.is_inactive(id))
                {
                    self.expanded_id = None;
                }
            }
            NotificationsAction::Upsert(notification) => {
                if let Some(existing) = self
                    .items
                    .iter_mut()
                    .find(|item| item.id == notification.id)
                {
                    *existing = notification;
                } else {
                    self.items.push(notification);
                }
                self.items = sort_notifications(std::mem::take(&mut self.items));
                self.items.truncate(MAX_NOTIFICATIONS);
                self.selected_index = self
                    .selected_index
                    .min(self.active_items().len().saturating_sub(1));
                if self
                    .expanded_id
                    .as_deref()
                    .is_some_and(|id| self.item_by_id(id).is_none() || self.is_inactive(id))
                {
                    self.expanded_id = None;
                }
            }
            NotificationsAction::Select(index) => {
                self.selected_index = index.min(self.active_items().len().saturating_sub(1));
            }
            NotificationsAction::Navigate(delta) => {
                let max = self.active_items().len().saturating_sub(1);
                if delta > 0 {
                    self.selected_index = (self.selected_index + delta as usize).min(max);
                } else {
                    self.selected_index = self.selected_index.saturating_sub((-delta) as usize);
                }
            }
            NotificationsAction::ToggleExpand(id) => {
                if self.expanded_id.as_deref() == Some(id.as_str()) {
                    self.expanded_id = None;
                } else {
                    self.expanded_id = Some(id);
                }
            }
        }
    }

    pub fn all_items(&self) -> &[InboxNotification] {
        &self.items
    }

    pub fn active_items(&self) -> Vec<&InboxNotification> {
        self.items
            .iter()
            .filter(|item| item.archived_at.is_none() && item.deleted_at.is_none())
            .collect()
    }

    pub fn unread_count(&self) -> usize {
        self.active_items()
            .into_iter()
            .filter(|item| item.read_at.is_none())
            .count()
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn selected_item(&self) -> Option<&InboxNotification> {
        self.active_items().get(self.selected_index).copied()
    }

    pub fn expanded_id(&self) -> Option<&str> {
        self.expanded_id.as_deref()
    }

    pub fn selected_actions(&self) -> &[InboxNotificationAction] {
        self.selected_item()
            .map(|item| item.actions.as_slice())
            .unwrap_or(&[])
    }

    fn item_by_id(&self, id: &str) -> Option<&InboxNotification> {
        self.items.iter().find(|item| item.id == id)
    }

    fn is_inactive(&self, id: &str) -> bool {
        self.item_by_id(id)
            .is_some_and(|item| item.archived_at.is_some() || item.deleted_at.is_some())
    }
}

impl Default for NotificationsState {
    fn default() -> Self {
        Self::new()
    }
}

fn sort_notifications(mut items: Vec<InboxNotification>) -> Vec<InboxNotification> {
    items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(id: &str, updated_at: i64) -> InboxNotification {
        InboxNotification {
            id: id.to_string(),
            source: "plugin_auth".to_string(),
            kind: "plugin_needs_reconnect".to_string(),
            title: format!("Notification {id}"),
            body: "body".to_string(),
            subtitle: None,
            severity: "warning".to_string(),
            created_at: updated_at,
            updated_at,
            read_at: None,
            archived_at: None,
            deleted_at: None,
            actions: Vec::new(),
            metadata_json: None,
        }
    }

    #[test]
    fn replace_sorts_newest_first() {
        let mut state = NotificationsState::new();
        state.reduce(NotificationsAction::Replace(vec![
            notification("older", 10),
            notification("newer", 20),
        ]));

        assert_eq!(state.active_items()[0].id, "newer");
        assert_eq!(state.active_items()[1].id, "older");
    }

    #[test]
    fn unread_count_skips_archived_entries() {
        let mut archived = notification("archived", 20);
        archived.archived_at = Some(21);

        let mut state = NotificationsState::new();
        state.reduce(NotificationsAction::Replace(vec![
            notification("active", 10),
            archived,
        ]));

        assert_eq!(state.unread_count(), 1);
        assert_eq!(state.active_items().len(), 1);
    }
}
