use super::*;

impl TuiModel {
    pub(crate) fn export_thread(&mut self) {
        let Some(thread_id) = self
            .chat
            .active_thread()
            .map(|thread| thread.id.clone())
            .filter(|id| !id.trim().is_empty())
        else {
            self.status_line = "No thread to export".to_string();
            return;
        };
        self.send_daemon_command(DaemonCommand::ExportThread {
            thread_id: thread_id.clone(),
        });
        self.status_line = format!("Exporting thread {thread_id}...");
    }
}
