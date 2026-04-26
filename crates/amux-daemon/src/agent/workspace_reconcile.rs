use super::*;
use amux_protocol::WorkspaceOperator;

impl AgentEngine {
    pub(crate) fn spawn_svarog_workspace_reconciliation(engine: Arc<Self>) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        tokio::spawn(async move {
            if let Err(error) = engine.reconcile_svarog_workspace_operator_tasks().await {
                tracing::warn!(error = %error, "failed to reconcile Svarog workspace tasks");
            }
        });
    }

    pub async fn reconcile_svarog_workspace_operator_tasks(&self) -> Result<()> {
        let settings = self.history.list_workspace_settings().await?;
        for settings in settings
            .into_iter()
            .filter(|settings| settings.operator == WorkspaceOperator::Svarog)
        {
            self.start_svarog_workspace_operator_tasks(&settings.workspace_id)
                .await?;
        }
        Ok(())
    }
}
