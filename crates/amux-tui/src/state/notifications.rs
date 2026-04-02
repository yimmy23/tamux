use amux_protocol::{InboxNotification, InboxNotificationAction};

const MAX_NOTIFICATIONS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationsHeaderAction {
    MarkAllRead,
    ArchiveRead,
    Close,
}

#[derive(Debug, Clone)]
pub enum NotificationsAction {
    Replace(Vec<InboxNotification>),
    Upsert(InboxNotification),
    Select(usize),
    Navigate(i32),
    ToggleExpand(String),
    FocusHeader(Option<NotificationsHeaderAction>),
    StepHeader(i32),
    FocusRowAction(Option<usize>),
    StepRowAction(i32),
}

pub struct NotificationsState {
    items: Vec<InboxNotification>,
    selected_index: usize,
    expanded_id: Option<String>,
    header_action: Option<NotificationsHeaderAction>,
    row_action_index: Option<usize>,
}

impl NotificationsState {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected_index: 0,
            expanded_id: None,
            header_action: None,
            row_action_index: None,
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
                self.normalize_header_action();
                self.normalize_row_action();
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
                self.normalize_header_action();
                self.normalize_row_action();
            }
            NotificationsAction::Select(index) => {
                self.selected_index = index.min(self.active_items().len().saturating_sub(1));
                self.normalize_row_action();
            }
            NotificationsAction::Navigate(delta) => {
                let max = self.active_items().len().saturating_sub(1);
                if delta > 0 {
                    self.selected_index = (self.selected_index + delta as usize).min(max);
                } else {
                    self.selected_index = self.selected_index.saturating_sub((-delta) as usize);
                }
                self.normalize_row_action();
            }
            NotificationsAction::ToggleExpand(id) => {
                if self.expanded_id.as_deref() == Some(id.as_str()) {
                    self.expanded_id = None;
                } else {
                    self.expanded_id = Some(id);
                }
            }
            NotificationsAction::FocusHeader(action) => {
                self.header_action = action.filter(|action| self.is_header_action_enabled(*action));
                if self.header_action.is_some() {
                    self.row_action_index = None;
                }
            }
            NotificationsAction::StepHeader(delta) => {
                let actions = self.enabled_header_actions();
                if actions.is_empty() {
                    self.header_action = None;
                } else if self.header_action.is_none() {
                    self.header_action = Some(if delta < 0 {
                        actions[actions.len().saturating_sub(1)]
                    } else {
                        actions[0]
                    });
                } else {
                    let current = self
                        .header_action
                        .and_then(|action| {
                            actions.iter().position(|candidate| *candidate == action)
                        })
                        .unwrap_or(0);
                    let next = if delta < 0 {
                        current.saturating_sub((-delta) as usize)
                    } else {
                        (current + delta as usize).min(actions.len().saturating_sub(1))
                    };
                    self.header_action = actions.get(next).copied();
                }
                if self.header_action.is_some() {
                    self.row_action_index = None;
                }
            }
            NotificationsAction::FocusRowAction(index) => {
                self.row_action_index = index.filter(|index| self.is_row_action_enabled(*index));
                if self.row_action_index.is_some() {
                    self.header_action = None;
                }
            }
            NotificationsAction::StepRowAction(delta) => {
                let actions = self.enabled_row_action_indices();
                if actions.is_empty() {
                    self.row_action_index = None;
                } else if self.row_action_index.is_none() {
                    self.row_action_index = Some(if delta < 0 {
                        actions[actions.len().saturating_sub(1)]
                    } else {
                        actions[0]
                    });
                } else {
                    let current = self
                        .row_action_index
                        .and_then(|index| actions.iter().position(|candidate| *candidate == index))
                        .unwrap_or(0);
                    let next = if delta < 0 {
                        current.saturating_sub((-delta) as usize)
                    } else {
                        (current + delta as usize).min(actions.len().saturating_sub(1))
                    };
                    self.row_action_index = actions.get(next).copied();
                }
                if self.row_action_index.is_some() {
                    self.header_action = None;
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

    pub fn selected_header_action(&self) -> Option<NotificationsHeaderAction> {
        self.header_action
    }

    pub fn selected_row_action_index(&self) -> Option<usize> {
        self.row_action_index
    }

    pub fn first_enabled_header_action(&self) -> Option<NotificationsHeaderAction> {
        self.enabled_header_actions().into_iter().next()
    }

    pub fn first_enabled_row_action_index(&self) -> Option<usize> {
        self.enabled_row_action_indices().into_iter().next()
    }

    pub fn is_header_action_enabled(&self, action: NotificationsHeaderAction) -> bool {
        match action {
            NotificationsHeaderAction::MarkAllRead => self.unread_count() > 0,
            NotificationsHeaderAction::ArchiveRead => self
                .active_items()
                .into_iter()
                .any(|item| item.read_at.is_some()),
            NotificationsHeaderAction::Close => true,
        }
    }

    pub fn selected_actions(&self) -> &[InboxNotificationAction] {
        self.selected_item()
            .map(|item| item.actions.as_slice())
            .unwrap_or(&[])
    }

    pub fn is_row_action_enabled(&self, index: usize) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        match index {
            0 => true,
            1 => item.read_at.is_none(),
            2 | 3 => true,
            other => other < row_action_count(item),
        }
    }

    fn item_by_id(&self, id: &str) -> Option<&InboxNotification> {
        self.items.iter().find(|item| item.id == id)
    }

    fn is_inactive(&self, id: &str) -> bool {
        self.item_by_id(id)
            .is_some_and(|item| item.archived_at.is_some() || item.deleted_at.is_some())
    }

    fn enabled_header_actions(&self) -> Vec<NotificationsHeaderAction> {
        [
            NotificationsHeaderAction::MarkAllRead,
            NotificationsHeaderAction::ArchiveRead,
            NotificationsHeaderAction::Close,
        ]
        .into_iter()
        .filter(|action| self.is_header_action_enabled(*action))
        .collect()
    }

    fn enabled_row_action_indices(&self) -> Vec<usize> {
        let Some(item) = self.selected_item() else {
            return Vec::new();
        };
        (0..row_action_count(item))
            .filter(|index| self.is_row_action_enabled(*index))
            .collect()
    }

    fn normalize_header_action(&mut self) {
        if self
            .header_action
            .is_some_and(|action| !self.is_header_action_enabled(action))
        {
            self.header_action = self.first_enabled_header_action();
        }
    }

    fn normalize_row_action(&mut self) {
        if self
            .row_action_index
            .is_some_and(|index| !self.is_row_action_enabled(index))
        {
            self.row_action_index = self.first_enabled_row_action_index();
        }
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

fn row_action_count(item: &InboxNotification) -> usize {
    4 + item.actions.len()
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

    #[test]
    fn step_header_skips_disabled_actions() {
        let mut state = NotificationsState::new();
        state.reduce(NotificationsAction::Replace(vec![notification(
            "active", 10,
        )]));

        state.reduce(NotificationsAction::StepHeader(1));
        assert_eq!(
            state.selected_header_action(),
            Some(NotificationsHeaderAction::MarkAllRead)
        );

        state.reduce(NotificationsAction::StepHeader(1));
        assert_eq!(
            state.selected_header_action(),
            Some(NotificationsHeaderAction::Close)
        );
    }

    #[test]
    fn step_row_action_skips_disabled_read_button() {
        let mut read_notification = notification("active", 10);
        read_notification.read_at = Some(11);

        let mut state = NotificationsState::new();
        state.reduce(NotificationsAction::Replace(vec![read_notification]));

        state.reduce(NotificationsAction::StepRowAction(1));
        assert_eq!(state.selected_row_action_index(), Some(0));

        state.reduce(NotificationsAction::StepRowAction(1));
        assert_eq!(state.selected_row_action_index(), Some(2));
    }
}
