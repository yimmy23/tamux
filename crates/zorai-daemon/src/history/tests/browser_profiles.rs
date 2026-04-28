use super::*;

#[tokio::test]
async fn browser_profile_persistence_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let profile = crate::agent::types::BrowserProfile {
        profile_id: "browser-profile-main".to_string(),
        label: "Main Browser".to_string(),
        profile_dir: "/tmp/zorai/browser/main".to_string(),
        created_at: 1_777_230_000,
        updated_at: 1_777_230_100,
        last_used_at: Some(1_777_230_200),
    };

    store.upsert_browser_profile(&profile).await?;

    let row = store
        .get_browser_profile("browser-profile-main")
        .await?
        .expect("browser profile should exist");

    assert_eq!(
        row,
        BrowserProfileRow {
            profile_id: "browser-profile-main".to_string(),
            label: "Main Browser".to_string(),
            profile_dir: "/tmp/zorai/browser/main".to_string(),
            created_at: 1_777_230_000,
            updated_at: 1_777_230_100,
            last_used_at: Some(1_777_230_200),
        }
    );

    fs::remove_dir_all(root)?;
    Ok(())
}
