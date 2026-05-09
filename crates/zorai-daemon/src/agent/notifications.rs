use super::*;
use zorai_protocol::InboxNotification;

impl AgentEngine {
    pub(super) async fn upsert_inbox_notification(
        &self,
        notification: InboxNotification,
    ) -> Result<()> {
        self.history.upsert_notification(&notification).await?;
        let notification = self
            .history
            .get_notification_by_id(&notification.id)
            .await?
            .unwrap_or(notification);
        let _ = self
            .event_tx
            .send(AgentEvent::NotificationInboxUpsert { notification });
        Ok(())
    }
}
