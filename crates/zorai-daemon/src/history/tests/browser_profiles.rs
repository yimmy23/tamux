use super::*;

#[tokio::test]
async fn browser_profile_persistence_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let profile = crate::agent::types::BrowserProfile {
        profile_id: "browser-profile-main".to_string(),
        label: "Main Browser".to_string(),
        profile_dir: "/tmp/zorai/browser/main".to_string(),
        browser_kind: Some("chrome".to_string()),
        workspace_id: None,
        health_state: crate::agent::types::BrowserProfileHealth::Healthy,
        created_at: 1_777_230_000,
        updated_at: 1_777_230_100,
        last_used_at: Some(1_777_230_200),
        last_auth_success_at: None,
        last_auth_failure_at: None,
        last_auth_failure_reason: None,
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
            browser_kind: Some("chrome".to_string()),
            workspace_id: None,
            health_state: "healthy".to_string(),
            created_at: 1_777_230_000,
            updated_at: 1_777_230_100,
            last_used_at: Some(1_777_230_200),
            last_auth_success_at: None,
            last_auth_failure_at: None,
            last_auth_failure_reason: None,
        }
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn browser_profile_list_and_delete() -> Result<()> {
    let (store, root) = make_test_store().await?;

    for i in 0..3 {
        let profile = crate::agent::types::BrowserProfile {
            profile_id: format!("profile-{i}"),
            label: format!("Browser {i}"),
            profile_dir: format!("/tmp/zorai/browser/{i}"),
            browser_kind: Some("chrome".to_string()),
            workspace_id: None,
            health_state: crate::agent::types::BrowserProfileHealth::Healthy,
            created_at: 1_777_230_000 + i as u64,
            updated_at: 1_777_230_100 + i as u64,
            last_used_at: None,
            last_auth_success_at: None,
            last_auth_failure_at: None,
            last_auth_failure_reason: None,
        };
        store.upsert_browser_profile(&profile).await?;
    }

    let all = store.list_browser_profiles().await?;
    assert_eq!(all.len(), 3, "should list all profiles");

    store.delete_browser_profile("profile-1").await?;
    let after_delete = store.list_browser_profiles().await?;
    assert_eq!(after_delete.len(), 2, "should have one less after delete");

    let deleted = store.get_browser_profile("profile-1").await?;
    assert!(deleted.is_none(), "deleted profile should not exist");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn browser_profile_health_state_transitions() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let profile = crate::agent::types::BrowserProfile {
        profile_id: "health-test".to_string(),
        label: "Health Test".to_string(),
        profile_dir: "/tmp/zorai/browser/health".to_string(),
        browser_kind: Some("chromium".to_string()),
        workspace_id: None,
        health_state: crate::agent::types::BrowserProfileHealth::Healthy,
        created_at: 1_777_230_000,
        updated_at: 1_777_230_000,
        last_used_at: None,
        last_auth_success_at: None,
        last_auth_failure_at: None,
        last_auth_failure_reason: None,
    };
    store.upsert_browser_profile(&profile).await?;

    // Transition to stale
    let stale = crate::agent::types::BrowserProfile {
        health_state: crate::agent::types::BrowserProfileHealth::Stale,
        updated_at: 1_777_230_500,
        ..profile.clone()
    };
    store.upsert_browser_profile(&stale).await?;
    let row = store.get_browser_profile("health-test").await?.unwrap();
    assert_eq!(row.health_state, "stale");

    // Transition to repair_needed
    let repair = crate::agent::types::BrowserProfile {
        health_state: crate::agent::types::BrowserProfileHealth::RepairNeeded,
        updated_at: 1_777_231_000,
        last_auth_failure_at: Some(1_777_230_900),
        last_auth_failure_reason: Some("cookie jar corrupted".to_string()),
        ..stale
    };
    store.upsert_browser_profile(&repair).await?;
    let row = store.get_browser_profile("health-test").await?.unwrap();
    assert_eq!(row.health_state, "repair_needed");
    assert_eq!(
        row.last_auth_failure_reason.as_deref(),
        Some("cookie jar corrupted")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn browser_profile_expiry_detection_and_repair_flow() -> Result<()> {
    // Proves the Slice 6 DoD requirement: simulate expiry → repair → reuse
    let (store, root) = make_test_store().await?;

    // Reference time: "now"
    let now_ms: u64 = 1_800_000_000_000;
    // Old timestamps: 60 days ago (well past the 30-day expiry threshold)
    let sixty_days_ago = now_ms.saturating_sub(60 * 24 * 60 * 60 * 1000);

    // Create a profile that should be expired (last_used_at is 60 days old)
    let old_profile = crate::agent::types::BrowserProfile {
        profile_id: "expired-work".to_string(),
        label: "Expired Work Profile".to_string(),
        profile_dir: "/tmp/zorai/browser/expired-work".to_string(),
        browser_kind: Some("chrome".to_string()),
        workspace_id: None,
        health_state: crate::agent::types::BrowserProfileHealth::Healthy,
        created_at: sixty_days_ago,
        updated_at: sixty_days_ago,
        last_used_at: Some(sixty_days_ago),
        last_auth_success_at: Some(sixty_days_ago),
        last_auth_failure_at: None,
        last_auth_failure_reason: None,
    };
    store.upsert_browser_profile(&old_profile).await?;

    // Verify it starts as healthy
    let row = store.get_browser_profile("expired-work").await?.unwrap();
    assert_eq!(row.health_state, "healthy", "should start healthy");

    // Step 1: Run expiry detection — should classify as expired
    let reclassified = store.detect_and_classify_expired_profiles(now_ms).await?;
    assert_eq!(reclassified.len(), 1, "one profile should be reclassified");
    assert_eq!(reclassified[0].0, "expired-work");
    assert_eq!(reclassified[0].1, "healthy");
    assert_eq!(reclassified[0].2, "expired");
    assert!(
        reclassified[0].3.contains("exceeds"),
        "reason should mention threshold"
    );

    // Verify the health state was updated in the database
    let row = store.get_browser_profile("expired-work").await?.unwrap();
    assert_eq!(row.health_state, "expired", "should now be expired");

    // Step 2: Repair — manually update to healthy (simulating user re-auth)
    let repaired = crate::agent::types::BrowserProfile {
        profile_id: "expired-work".to_string(),
        label: "Expired Work Profile".to_string(),
        profile_dir: "/tmp/zorai/browser/expired-work".to_string(),
        browser_kind: Some("chrome".to_string()),
        workspace_id: None,
        health_state: crate::agent::types::BrowserProfileHealth::Healthy,
        created_at: sixty_days_ago,
        updated_at: now_ms,
        last_used_at: Some(now_ms),
        last_auth_success_at: Some(now_ms),
        last_auth_failure_at: None,
        last_auth_failure_reason: None,
    };
    store.upsert_browser_profile(&repaired).await?;

    // Verify it's healthy again
    let row = store.get_browser_profile("expired-work").await?.unwrap();
    assert_eq!(
        row.health_state, "healthy",
        "should be healthy after repair"
    );

    // Step 3: Reuse — run expiry detection again, should NOT reclassify
    let reclassified = store.detect_and_classify_expired_profiles(now_ms).await?;
    assert!(
        reclassified.is_empty(),
        "repaired profile should not be reclassified"
    );

    // Step 4: Verify it appears in list_browser_profiles
    let all = store.list_browser_profiles().await?;
    let found = all
        .iter()
        .find(|p| p.profile_id == "expired-work")
        .expect("repaired profile should still be listed");
    assert_eq!(found.health_state, "healthy");
    assert_eq!(found.last_used_at, Some(now_ms));
    assert_eq!(found.last_auth_success_at, Some(now_ms));

    fs::remove_dir_all(root)?;
    Ok(())
}
