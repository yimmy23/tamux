use super::*;

impl TuiModel {
    pub(crate) fn export_thread(&mut self, index: usize) {
        let (thread_id, message_id) = {
            let Some(thread) = self.chat.active_thread() else {
                self.status_line = "No thread to export".to_string();
                return;
            };
            if index >= thread.messages.len() {
                return;
            }
            let Some(message_id) = thread.messages[index]
                .id
                .clone()
                .filter(|id| !id.trim().is_empty())
            else {
                self.status_line = "Cannot export before the message is saved".to_string();
                return;
            };
            if thread.id.trim().is_empty() {
                self.status_line = "No thread to export".to_string();
                return;
            }
            (thread.id.clone(), message_id)
        };
        self.send_daemon_command(DaemonCommand::ExportThread {
            thread_id: thread_id.clone(),
            message_id,
        });
        self.status_line = format!("Exporting thread {thread_id} through message {}...", index + 1);
    }
}
