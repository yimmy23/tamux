use semver::Version;

use crate::InboxNotification;

pub const TAMUX_NPM_PACKAGE: &str = "tamux";
pub const TAMUX_NPM_LATEST_URL: &str = "https://registry.npmjs.org/tamux/latest";
pub const TAMUX_UPDATE_NOTIFICATION_ID: &str = "tamux-update-available";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TamuxUpdateStatus {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
}

#[derive(serde::Deserialize)]
struct NpmLatestResponse {
    version: String,
}

impl TamuxUpdateStatus {
    pub fn from_versions(current: &str, latest: &str) -> Option<Self> {
        let current = parse_version(current)?;
        let latest = parse_version(latest)?;

        Some(Self {
            current_version: current.to_string(),
            latest_version: latest.to_string(),
            update_available: latest > current,
        })
    }

    pub fn cli_notice(&self) -> Option<String> {
        self.update_available.then(|| {
            format!(
                "Update available: tamux {} -> {}. Run `tamux upgrade` to install the latest tamux release.",
                self.current_version, self.latest_version
            )
        })
    }

    pub fn into_notification(self, timestamp_ms: i64) -> InboxNotification {
        let (title, body, archived_at, deleted_at, read_at) = if self.update_available {
            (
                format!("tamux {} is available", self.latest_version),
                format!(
                    "Installed: {}. Latest: {}. Run `tamux upgrade` to install the latest tamux release.",
                    self.current_version, self.latest_version
                ),
                None,
                None,
                None,
            )
        } else {
            (
                "tamux is up to date".to_string(),
                format!(
                    "Installed version {} matches the latest available tamux release.",
                    self.current_version
                ),
                Some(timestamp_ms),
                Some(timestamp_ms),
                Some(timestamp_ms),
            )
        };

        InboxNotification {
            id: TAMUX_UPDATE_NOTIFICATION_ID.to_string(),
            source: "tamux_update".to_string(),
            kind: "version_update".to_string(),
            title,
            body,
            subtitle: Some(format!(
                "current {} | latest {}",
                self.current_version, self.latest_version
            )),
            severity: "info".to_string(),
            created_at: timestamp_ms,
            updated_at: timestamp_ms,
            read_at,
            archived_at,
            deleted_at,
            actions: Vec::new(),
            metadata_json: Some(
                serde_json::json!({
                    "current_version": self.current_version,
                    "latest_version": self.latest_version,
                    "update_available": self.update_available,
                })
                .to_string(),
            ),
        }
    }
}

pub fn parse_npm_latest_version(body: &str) -> Option<String> {
    let response: NpmLatestResponse = serde_json::from_str(body).ok()?;
    Some(parse_version(&response.version)?.to_string())
}

fn parse_version(raw: &str) -> Option<Version> {
    Version::parse(raw.trim().trim_start_matches('v')).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_when_latest_version_is_newer() {
        let status = TamuxUpdateStatus::from_versions("0.2.3", "0.2.4")
            .expect("status should parse valid semver versions");

        assert!(status.update_available);
        assert_eq!(status.current_version, "0.2.3");
        assert_eq!(status.latest_version, "0.2.4");
    }

    #[test]
    fn accepts_versions_with_v_prefix() {
        let status = TamuxUpdateStatus::from_versions("v0.2.3", "v0.2.4")
            .expect("status should normalize v-prefixed versions");

        assert!(status.update_available);
        assert_eq!(status.current_version, "0.2.3");
        assert_eq!(status.latest_version, "0.2.4");
    }

    #[test]
    fn reports_up_to_date_when_versions_match() {
        let status = TamuxUpdateStatus::from_versions("0.2.4", "0.2.4")
            .expect("status should parse matching versions");

        assert!(!status.update_available);
    }

    #[test]
    fn builds_active_upgrade_notification_when_update_exists() {
        let status = TamuxUpdateStatus::from_versions("0.2.3", "0.2.4")
            .expect("status should parse valid semver versions");

        let notification = status.into_notification(42);

        assert_eq!(notification.id, TAMUX_UPDATE_NOTIFICATION_ID);
        assert_eq!(notification.source, "tamux_update");
        assert_eq!(notification.title, "tamux 0.2.4 is available");
        assert!(
            notification.body.contains("Run `tamux upgrade`"),
            "notification should direct the operator to the upgrade command"
        );
        assert!(
            !notification.body.contains("npm"),
            "notification body should stay installation-source agnostic"
        );
        assert_eq!(notification.archived_at, None);
        assert_eq!(notification.deleted_at, None);
    }

    #[test]
    fn cli_notice_stays_installation_source_agnostic() {
        let status = TamuxUpdateStatus::from_versions("0.2.3", "0.2.4")
            .expect("status should parse valid semver versions");

        let notice = status.cli_notice().expect("notice");

        assert!(notice.contains("Run `tamux upgrade`"));
        assert!(!notice.contains("npm"));
        assert!(!notice.contains("tamux@latest"));
    }

    #[test]
    fn archives_upgrade_notification_when_already_current() {
        let status = TamuxUpdateStatus::from_versions("0.2.4", "0.2.4")
            .expect("status should parse matching versions");

        let notification = status.into_notification(84);

        assert_eq!(notification.id, TAMUX_UPDATE_NOTIFICATION_ID);
        assert_eq!(notification.archived_at, Some(84));
        assert_eq!(notification.deleted_at, Some(84));
    }
}
