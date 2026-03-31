use super::*;

#[test]
fn test_select_list_wraps_index() {
    let len = 4usize;
    let mut idx = 0usize;
    if idx == 0 {
        idx = len.saturating_sub(1);
    } else {
        idx -= 1;
    }
    assert_eq!(idx, 3);

    idx = 3;
    idx += 1;
    if idx >= len {
        idx = 0;
    }
    assert_eq!(idx, 0);

    idx = 1;
    idx += 1;
    if idx >= len {
        idx = 0;
    }
    assert_eq!(idx, 2);
}

#[test]
fn test_is_local_provider() {
    assert!(is_local_provider("ollama"));
    assert!(is_local_provider("lmstudio"));
    assert!(!is_local_provider("anthropic"));
    assert!(!is_local_provider("openai"));
}

#[test]
fn test_security_default_for_tier() {
    assert_eq!(default_security_index("newcomer"), 0);
    assert_eq!(default_security_index("familiar"), 1);
    assert_eq!(default_security_index("power_user"), 2);
    assert_eq!(default_security_index("expert"), 2);
    assert_eq!(default_security_index("unknown"), 1);
}

#[test]
fn test_tier_shows_optional_steps() {
    assert!(!tier_shows_step("newcomer", "model"));
    assert!(!tier_shows_step("newcomer", "data_dir"));
    assert!(tier_shows_step("familiar", "model"));
    assert!(tier_shows_step("familiar", "data_dir"));
    assert!(tier_shows_step("power_user", "model"));
    assert!(tier_shows_step("expert", "model"));
}

#[test]
fn test_security_level_from_index() {
    assert_eq!(
        security_level_from_index(0),
        ("highest", "Approve risky actions")
    );
    assert_eq!(
        security_level_from_index(1),
        ("moderate", "Approve risky actions")
    );
    assert_eq!(
        security_level_from_index(2),
        ("lowest", "Approve destructive only")
    );
    assert_eq!(
        security_level_from_index(3),
        ("yolo", "Minimize interruptions")
    );
    assert_eq!(
        security_level_from_index(99),
        ("moderate", "Approve risky actions")
    );
}

#[test]
fn test_post_setup_action_from_index() {
    assert_eq!(post_setup_action_from_index(0), PostSetupAction::LaunchTui);
    assert_eq!(
        post_setup_action_from_index(1),
        PostSetupAction::LaunchElectron
    );
    assert_eq!(post_setup_action_from_index(2), PostSetupAction::NotNow);
}

#[test]
fn test_post_setup_choices_include_not_now() {
    let choices = post_setup_choices();
    assert_eq!(choices.len(), 3);
    assert_eq!(choices[0].0, "TUI");
    assert_eq!(choices[1].0, "Electron");
    assert_eq!(choices[2].0, "Not now");
}

#[test]
fn test_gateway_choice_items_include_whatsapp_and_skip() {
    let items = gateway_choice_items();
    assert_eq!(items.len(), 5);
    assert_eq!(items[3], ("WhatsApp", "whatsapp"));
    assert_eq!(items[4], ("Skip", ""));
}

#[test]
fn test_whatsapp_timeout_choice_mapping() {
    let choices = whatsapp_timeout_choices();
    assert_eq!(choices.len(), 2);
    assert!(whatsapp_timeout_retry_selected(0));
    assert!(!whatsapp_timeout_retry_selected(1));
}

#[test]
fn whatsapp_setup_accepts_multiline_or_csv_contacts() {
    let parsed = parse_whatsapp_setup_allowlist("+48 123 456 789, 15551230000\n+44 20 7946 0958");

    assert_eq!(
        parsed,
        Some(vec![
            "48123456789".to_string(),
            "15551230000".to_string(),
            "442079460958".to_string(),
        ])
    );
}

#[test]
fn whatsapp_setup_rejects_empty_allowlist() {
    assert_eq!(
        parse_whatsapp_setup_allowlist("\n , invalid , device "),
        None
    );
    assert_eq!(parse_whatsapp_setup_allowlist("   \n  "), None);
}

#[test]
fn whatsapp_setup_cancellation_paths_stop_without_retry() {
    assert_eq!(
        resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::Cancelled),
        WhatsAppAllowlistPromptResolution::Cancel
    );
    assert_eq!(
        resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::EndOfInput),
        WhatsAppAllowlistPromptResolution::Cancel
    );
}

#[test]
fn whatsapp_setup_invalid_submission_requests_retry() {
    assert_eq!(
        resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::Submitted(
            "\n , invalid , device ".to_string()
        )),
        WhatsAppAllowlistPromptResolution::Retry(
            "Enter at least one valid WhatsApp phone number before linking."
        )
    );
}

#[test]
fn whatsapp_setup_persists_allowlist_before_gateway_enable() {
    let writes = whatsapp_gateway_config_writes("+48 123 456 789").expect("valid writes");

    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].key_path, "/gateway/whatsapp_allowed_contacts");
    assert_eq!(writes[0].value_json, "\"+48 123 456 789\"");
    assert_eq!(writes[1].key_path, "/gateway/enabled");
    assert_eq!(writes[1].value_json, "true");
}
