use super::*;
impl TuiModel {
    pub(super) fn activate_database_settings_field(&mut self, field: &str) -> bool {
        match field {
            "db_sync_now" => {
                self.send_daemon_command(DaemonCommand::DatabaseSyncNow);
                self.status_line = "Syncing database…".to_string();
                true
            }
            _ => false,
        }
    }
}
