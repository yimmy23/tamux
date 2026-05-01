use crate::plugin::manager_extras::auth_status_from_expiry_and_refresh_token;
use crate::plugin::PluginAuthStatus;

#[test]
fn auth_status_uses_expiring_soon_window() {
    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
    let status = auth_status_from_expiry_and_refresh_token(Some(expires_at.as_str()), false);
    assert_eq!(status, PluginAuthStatus::ExpiringSoon);
}
