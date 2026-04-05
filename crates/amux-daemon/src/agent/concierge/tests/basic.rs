use super::*;
use crate::agent::operator_profile;

#[test]
fn welcome_signature_changes_when_context_changes() {
    let base_context = WelcomeContext {
        recent_threads: vec![ThreadSummary {
            id: "t1".to_string(),
            title: "Thread One".to_string(),
            updated_at: 100,
            message_count: 3,
            opening_message: Some("User: kickoff".to_string()),
            last_messages: vec!["hello".to_string()],
        }],
        pending_task_total: 1,
        pending_tasks: vec!["task-a".to_string()],
    };
    let mut changed_context = WelcomeContext {
        recent_threads: vec![ThreadSummary {
            id: "t1".to_string(),
            title: "Thread One".to_string(),
            updated_at: 100,
            message_count: 3,
            opening_message: Some("User: kickoff".to_string()),
            last_messages: vec!["hello".to_string()],
        }],
        pending_task_total: 1,
        pending_tasks: vec!["task-a".to_string()],
    };
    changed_context.pending_tasks.push("task-b".to_string());

    let a = build_welcome_signature(ConciergeDetailLevel::ProactiveTriage, &base_context);
    let b = build_welcome_signature(ConciergeDetailLevel::ProactiveTriage, &changed_context);
    assert_ne!(a, b);
}

#[test]
fn gateway_triage_prompt_includes_recent_channel_history_when_present() {
    let context = WelcomeContext {
        recent_threads: vec![],
        pending_task_total: 0,
        pending_tasks: vec![],
    };

    let prompt = build_gateway_triage_prompt(
        "WhatsApp",
        "alice",
        "What did we just discuss?",
        Some("- user: we discussed backlog\n- assistant: we discussed replay"),
        &context,
    );

    assert!(prompt.contains("[WhatsApp message from alice]: What did we just discuss?"));
    assert!(prompt.contains("Recent messages from this same channel:"));
    assert!(prompt.contains("- user: we discussed backlog"));
    assert!(prompt.contains("- assistant: we discussed replay"));
}

#[test]
fn gateway_triage_safe_tools_include_lookup_and_agent_coordination_tools() {
    let mut config = AgentConfig::default();
    config.tools.web_search = true;
    config.enable_honcho_memory = true;
    config.honcho_api_key = "test-key".to_string();

    let tool_names: Vec<String> =
        gateway_triage_safe_tools(&config, std::path::Path::new("/tmp/tamux-agent-test"), true)
            .into_iter()
            .map(|tool| tool.function.name)
            .collect();

    assert_eq!(
        tool_names,
        vec![
            "search_history",
            "fetch_gateway_history",
            "session_search",
            "agent_query_memory",
            "onecontext_search",
            "web_search",
            "message_agent",
        ]
    );
}

#[test]
fn resolve_concierge_provider_uses_shared_resolution_path() {
    let mut config = AgentConfig::default();
    config.provider = amux_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.base_url = "https://api.openai.com/v1".to_string();
    config.model = "gpt-5.4".to_string();
    config.reasoning_effort = "high".to_string();
    config.context_window_tokens = 123_000;
    config.assistant_id = "assistant-root".to_string();
    config.concierge.provider = Some(amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN.to_string());
    config.concierge.model = Some("qwen3.5-plus".to_string());
    config.concierge.reasoning_effort = Some("high".to_string());
    config.providers.insert(
        amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
        ProviderConfig {
            base_url: String::new(),
            model: String::new(),
            api_key: "dashscope-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::Responses,
            context_window_tokens: 0,
            reasoning_effort: String::new(),
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );

    let resolved = resolve_concierge_provider(&config).expect("concierge provider should resolve");
    let shared = resolve_provider_config_for(
        &config,
        amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN,
        Some("qwen3.5-plus"),
    )
    .expect("shared provider resolution should succeed");
    assert_eq!(resolved.base_url, shared.base_url);
    assert_eq!(resolved.model, shared.model);
    assert_eq!(resolved.api_key, shared.api_key);
    assert_eq!(resolved.reasoning_effort, "high");
    assert_eq!(resolved.assistant_id, shared.assistant_id);
    assert_eq!(resolved.context_window_tokens, shared.context_window_tokens);
    assert_eq!(resolved.api_transport, shared.api_transport);
}

#[test]
fn resolve_concierge_provider_defaults_reasoning_effort_to_off() {
    let mut config = AgentConfig::default();
    config.provider = amux_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.base_url = "https://api.openai.com/v1".to_string();
    config.model = "gpt-5.4".to_string();
    config.reasoning_effort = "high".to_string();

    let resolved = resolve_concierge_provider(&config).expect("concierge provider should resolve");
    assert_eq!(resolved.reasoning_effort, "off");
}

#[test]
fn concierge_fast_profile_preserves_selected_reasoning_without_touching_model() {
    let base = ProviderConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        model: "gpt-5.4-mini".to_string(),
        api_key: "secret".to_string(),
        assistant_id: "assistant-1".to_string(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
        context_window_tokens: 128_000,
        response_schema: None,
        stop_sequences: None,
        temperature: None,
        top_p: None,
        top_k: None,
        metadata: None,
        service_tier: None,
        container: None,
        inference_geo: None,
        cache_control: None,
        max_tokens: None,
        anthropic_tool_choice: None,
        output_effort: None,
    };

    let fast = fast_concierge_provider_config(&base);

    assert_eq!(fast.reasoning_effort, "high");
    assert_eq!(fast.model, base.model);
    assert_eq!(fast.base_url, base.base_url);
    assert_eq!(fast.api_transport, base.api_transport);
    assert_eq!(fast.context_window_tokens, base.context_window_tokens);
}

fn make_answered(keys: &[&str]) -> HashMap<String, operator_profile::ProfileFieldValue> {
    keys.iter()
        .map(|k| {
            (
                k.to_string(),
                operator_profile::ProfileFieldValue {
                    value_json: "\"test\"".to_string(),
                    confidence: 1.0,
                    source: "test".to_string(),
                    updated_at: 0,
                },
            )
        })
        .collect()
}

#[test]
fn profile_decision_with_empty_profile_emits_first_required_question() {
    let specs = operator_profile::default_field_specs();
    let answered = HashMap::new();
    let session = operator_profile::InterviewSession::new("sess-1", "first_run_onboarding");
    let decision = profile_interview_decision(&specs, &answered, &session, 0);

    match decision {
        WelcomeProfileDecision::EmitProfileQuestion {
            field_key,
            optional,
            ..
        } => {
            assert_eq!(field_key, "name");
            assert!(!optional);
        }
        WelcomeProfileDecision::StandardWelcome => {
            panic!("expected EmitProfileQuestion but got StandardWelcome");
        }
    }
}

#[test]
fn profile_decision_with_tier_done_but_profile_incomplete_emits_question() {
    let specs = operator_profile::default_field_specs();
    let optional_keys: Vec<&str> = specs
        .iter()
        .filter(|s| !s.required)
        .map(|s| s.field_key.as_str())
        .collect();
    let answered = make_answered(&optional_keys);
    let session = operator_profile::InterviewSession::new("sess-2", "first_run_onboarding");
    let decision = profile_interview_decision(&specs, &answered, &session, 0);

    assert!(matches!(
        decision,
        WelcomeProfileDecision::EmitProfileQuestion { .. }
    ));
}

#[test]
fn profile_decision_with_all_required_answered_returns_standard_welcome() {
    let specs = operator_profile::default_field_specs();
    let required_keys: Vec<&str> = specs
        .iter()
        .filter(|s| s.required)
        .map(|s| s.field_key.as_str())
        .collect();
    let answered = make_answered(&required_keys);
    let session = operator_profile::InterviewSession::new("sess-3", "first_run_onboarding");
    let decision = profile_interview_decision(&specs, &answered, &session, 0);

    assert_eq!(decision, WelcomeProfileDecision::StandardWelcome);
}

#[test]
fn profile_decision_standard_welcome_when_only_optional_fields_remain() {
    let specs = operator_profile::default_field_specs();
    let required_keys: Vec<&str> = specs
        .iter()
        .filter(|s| s.required)
        .map(|s| s.field_key.as_str())
        .collect();
    let answered = make_answered(&required_keys);
    let session = operator_profile::InterviewSession::new("sess-4", "first_run_onboarding");

    assert!(operator_profile::is_complete(&specs, &answered));
    let decision = profile_interview_decision(&specs, &answered, &session, 0);
    assert_eq!(decision, WelcomeProfileDecision::StandardWelcome);
}
