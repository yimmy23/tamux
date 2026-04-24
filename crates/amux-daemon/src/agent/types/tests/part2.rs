use amux_shared::providers::{
    PROVIDER_ID_ARCEE, PROVIDER_ID_CHUTES, PROVIDER_ID_DEEPSEEK, PROVIDER_ID_GITHUB_COPILOT,
    PROVIDER_ID_NVIDIA, PROVIDER_ID_XAI,
};

    #[test]
    fn custom_auth_yaml_hydrates_provider_definition_with_models() {
        let _lock = crate::test_support::env_test_lock();
        let _env_guard = crate::test_support::EnvGuard::new(&["TAMUX_CUSTOM_AUTH_PATH"]);
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let custom_auth_path = temp_dir.path().join("custom-auth.yaml");
        std::fs::write(
            &custom_auth_path,
            r#"
providers:
  - id: local-openai
    name: Local OpenAI-Compatible
    default_base_url: http://127.0.0.1:11434/v1
    default_model: llama3.3
    api_type: openai
    auth_method: bearer
    supports_model_fetch: true
    supported_transports: [chat_completions, responses]
    default_transport: chat_completions
    supports_response_continuity: true
    models:
      - id: llama3.3
        name: Llama 3.3
        context_window: 128000
        modalities: [text, image]
"#,
        )
        .expect("write custom auth");
        std::env::set_var("TAMUX_CUSTOM_AUTH_PATH", &custom_auth_path);

        let report = reload_custom_provider_catalog_from_default_path();

        assert_eq!(report.loaded_provider_count, 1);
        assert!(report.diagnostics.is_empty());
        let provider = get_provider_definition("local-openai").expect("custom provider");
        assert_eq!(provider.name, "Local OpenAI-Compatible");
        assert_eq!(provider.default_base_url, "http://127.0.0.1:11434/v1");
        assert_eq!(provider.default_model, "llama3.3");
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(
            provider.supported_transports,
            &[ApiTransport::ChatCompletions, ApiTransport::Responses]
        );
        assert!(provider.supports_model_fetch);
        assert!(provider.supports_response_continuity);
        assert_eq!(provider.models.len(), 1);
        assert_eq!(provider.models[0].id, "llama3.3");
        assert_eq!(provider.models[0].modalities, TEXT_IMAGE);
    }

    #[test]
    fn custom_auth_yaml_accepts_scalar_api_key_values() {
        let _lock = crate::test_support::env_test_lock();
        let _env_guard = crate::test_support::EnvGuard::new(&["TAMUX_CUSTOM_AUTH_PATH"]);
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let custom_auth_path = temp_dir.path().join("custom-auth.yaml");
        std::fs::write(
            &custom_auth_path,
            r#"
providers:
  - id: local-openai
    name: Local OpenAI-Compatible
    default_base_url: http://127.0.0.1:11434/v1
    default_model: llama3.3
    api_key: 1231
    models:
      - id: llama3.3
        context_window: 128000
"#,
        )
        .expect("write custom auth");
        std::env::set_var("TAMUX_CUSTOM_AUTH_PATH", &custom_auth_path);

        let report = reload_custom_provider_catalog_from_default_path();

        assert_eq!(report.loaded_provider_count, 1);
        assert!(report.diagnostics.is_empty());
        let provider_config = custom_provider_config("local-openai").expect("custom provider");
        assert_eq!(provider_config.api_key, "1231");
    }

    #[test]
    fn custom_auth_yaml_reports_invalid_entries_without_dropping_valid_providers() {
        let _lock = crate::test_support::env_test_lock();
        let _env_guard = crate::test_support::EnvGuard::new(&["TAMUX_CUSTOM_AUTH_PATH"]);
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let custom_auth_path = temp_dir.path().join("custom-auth.yaml");
        std::fs::write(
            &custom_auth_path,
            r#"
providers:
  - id: openai
    name: Should Not Override
    default_base_url: http://127.0.0.1:9999/v1
    default_model: nope
    models:
      - id: nope
        name: Nope
        context_window: 1000
  - id: custom-valid
    name: Custom Valid
    default_base_url: http://127.0.0.1:8080/v1
    default_model: custom-model
    models:
      - id: custom-model
        name: Custom Model
        context_window: 64000
"#,
        )
        .expect("write custom auth");
        std::env::set_var("TAMUX_CUSTOM_AUTH_PATH", &custom_auth_path);

        let report = reload_custom_provider_catalog_from_default_path();

        assert_eq!(report.loaded_provider_count, 1);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.provider_id.as_deref() == Some("openai")
                    && diagnostic.message.contains("built-in provider id")),
            "expected diagnostic for built-in provider collision, got {:?}",
            report.diagnostics
        );
        assert_eq!(
            get_provider_definition("openai")
                .expect("built-in openai")
                .default_base_url,
            "https://api.openai.com/v1"
        );
        assert!(get_provider_definition("custom-valid").is_some());
    }

    #[test]
    fn circuit_breaker_event_deserializes_richer_outage_metadata() {
        let json = serde_json::json!({
            "type": "provider_circuit_open",
            "provider": amux_shared::providers::PROVIDER_ID_OPENAI,
            "failed_model": "gpt-4o",
            "trip_count": 4,
            "reason": "circuit breaker open after repeated failures",
            "suggested_alternatives": [
                {
                    "provider_id": "groq",
                    "model": "llama-3.3-70b-versatile",
                    "reason": "healthy and configured"
                }
            ]
        });

        let parsed: AgentEvent = serde_json::from_value(json).unwrap();
        match parsed {
            AgentEvent::ProviderCircuitOpen {
                provider,
                failed_model,
                trip_count,
                reason,
                suggested_alternatives,
            } => {
                assert_eq!(provider, PROVIDER_ID_OPENAI);
                assert_eq!(failed_model.as_deref(), Some("gpt-4o"));
                assert_eq!(trip_count, 4);
                assert_eq!(reason, "circuit breaker open after repeated failures");
                assert_eq!(suggested_alternatives.len(), 1);
                assert_eq!(suggested_alternatives[0].provider_id, "groq");
                assert_eq!(
                    suggested_alternatives[0].model.as_deref(),
                    Some("llama-3.3-70b-versatile")
                );
            }
            _ => panic!("wrong variant after deserialize"),
        }
    }

    #[test]
    fn circuit_breaker_event_deserializes_legacy_shape_without_reason() {
        let json = serde_json::json!({
            "type": "provider_circuit_open",
            "provider": PROVIDER_ID_OPENAI,
            "trip_count": 2
        });

        let parsed: AgentEvent = serde_json::from_value(json).unwrap();
        match parsed {
            AgentEvent::ProviderCircuitOpen {
                provider,
                failed_model,
                trip_count,
                reason,
                suggested_alternatives,
            } => {
                assert_eq!(provider, PROVIDER_ID_OPENAI);
                assert!(failed_model.is_none());
                assert_eq!(trip_count, 2);
                assert_eq!(reason, "circuit breaker open");
                assert!(suggested_alternatives.is_empty());
            }
            _ => panic!("wrong variant after deserialize"),
        }
    }

    // ── Consolidation type tests (Phase 5 — MEMO-02/MEMO-07) ────────────

    #[test]
    fn consolidation_config_defaults() {
        let cfg = ConsolidationConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.budget_secs, 30);
        assert_eq!(cfg.idle_threshold_secs, 300);
        assert_eq!(cfg.tombstone_ttl_days, 7);
        assert_eq!(cfg.heuristic_promotion_threshold, 3);
        assert!((cfg.memory_decay_half_life_hours - 69.0).abs() < f64::EPSILON);
        assert!(!cfg.auto_resume_goal_runs);
        assert!((cfg.fact_decay_supersede_threshold - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn consolidation_config_deserializes_from_empty_json() {
        let cfg: ConsolidationConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.budget_secs, 30);
        assert_eq!(cfg.idle_threshold_secs, 300);
    }

    #[test]
    fn consolidation_config_on_agent_config() {
        let json = r#"{}"#;
        let parsed: AgentConfig = serde_json::from_str(json).unwrap();
        assert!(parsed.consolidation.enabled);
        assert_eq!(parsed.consolidation.budget_secs, 30);
    }

    #[test]
    fn consolidation_result_defaults_are_zero() {
        let result = ConsolidationResult::default();
        assert_eq!(result.traces_reviewed, 0);
        assert!(!result.distillation_ran);
        assert_eq!(result.distillation_threads_analyzed, 0);
        assert_eq!(result.distillation_candidates_generated, 0);
        assert_eq!(result.distillation_auto_applied, 0);
        assert_eq!(result.distillation_queued_for_review, 0);
        assert!(!result.forge_ran);
        assert_eq!(result.forge_traces_analyzed, 0);
        assert_eq!(result.forge_patterns_detected, 0);
        assert_eq!(result.forge_hints_generated, 0);
        assert_eq!(result.forge_hints_auto_applied, 0);
        assert_eq!(result.facts_decayed, 0);
        assert_eq!(result.tombstones_purged, 0);
        assert_eq!(result.facts_refined, 0);
        assert!(result.skipped_reason.is_none());
    }

    // -----------------------------------------------------------------------
    // Skill discovery type contract tests (SKIL-01, SKIL-02, SKIL-03, SKIL-05)
    // -----------------------------------------------------------------------

    #[test]
    fn skill_maturity_status_from_str_roundtrip() {
        let draft = serde_json::from_str::<SkillMaturityStatus>(r#""draft""#).unwrap();
        assert_eq!(draft, SkillMaturityStatus::Draft);
        let json = serde_json::to_string(&draft).unwrap();
        let roundtripped: SkillMaturityStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped, SkillMaturityStatus::Draft);
    }

    #[test]
    fn skill_maturity_status_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Draft).unwrap(),
            r#""draft""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Testing).unwrap(),
            r#""testing""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Active).unwrap(),
            r#""active""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Proven).unwrap(),
            r#""proven""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::PromotedToCanonical).unwrap(),
            r#""promoted_to_canonical""#
        );
    }

    #[test]
    fn skill_promotion_config_defaults() {
        let cfg = SkillPromotionConfig::default();
        assert_eq!(cfg.testing_to_active, 3);
        assert_eq!(cfg.active_to_proven, 5);
        assert_eq!(cfg.proven_to_canonical, 10);
    }

    #[test]
    fn skill_promotion_config_deserializes_from_empty_json() {
        let cfg: SkillPromotionConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.testing_to_active, 3);
        assert_eq!(cfg.active_to_proven, 5);
        assert_eq!(cfg.proven_to_canonical, 10);
    }

    #[test]
    fn skill_recommendation_config_deserializes_partial_json_with_defaults() {
        let json = serde_json::json!({
            "enabled": false,
            "strong_match_threshold": 0.91,
            "community_preapprove_timeout_secs": 45,
            "llm_semantic_search_on_no_match": false
        })
        .to_string();

        let cfg: SkillRecommendationConfig = serde_json::from_str(&json).unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.discovery_backend, "mesh");
        assert!(cfg.require_read_on_strong_match);
        assert!((cfg.strong_match_threshold - 0.91).abs() < f64::EPSILON);
        assert!((cfg.weak_match_threshold - 0.60).abs() < f64::EPSILON);
        assert!((cfg.novelty_distance_weight - 0.05).abs() < f64::EPSILON);
        assert!(cfg.background_community_search);
        assert_eq!(cfg.community_preapprove_timeout_secs, 45);
        assert_eq!(cfg.suggest_global_enable_after_approvals, 3);
        assert!(cfg.llm_normalize_on_no_match);
        assert!(!cfg.llm_semantic_search_on_no_match);
        assert_eq!(cfg.llm_semantic_search_max_skills, 64);
    }

    #[test]
    fn skill_discovery_config_defaults() {
        let cfg = SkillDiscoveryConfig::default();
        assert_eq!(cfg.min_tool_count, 8);
        assert_eq!(cfg.min_replan_count, 1);
        assert!((cfg.min_quality_score - 0.8).abs() < f64::EPSILON);
        assert!((cfg.novelty_similarity_threshold - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_config_deserializes_from_empty_json() {
        let cfg: SkillDiscoveryConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.min_tool_count, 8);
        assert_eq!(cfg.min_replan_count, 1);
    }

    #[test]
    fn skill_recommendation_config_defaults() {
        let cfg = SkillRecommendationConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.discovery_backend, "mesh");
        assert!(cfg.require_read_on_strong_match);
        assert!((cfg.strong_match_threshold - 0.85).abs() < f64::EPSILON);
        assert!((cfg.weak_match_threshold - 0.60).abs() < f64::EPSILON);
        assert!((cfg.novelty_distance_weight - 0.05).abs() < f64::EPSILON);
        assert!(cfg.background_community_search);
        assert_eq!(cfg.community_preapprove_timeout_secs, 30);
        assert_eq!(cfg.suggest_global_enable_after_approvals, 3);
        assert!(cfg.llm_normalize_on_no_match);
        assert!(cfg.llm_semantic_search_on_no_match);
        assert_eq!(cfg.llm_semantic_search_max_skills, 64);
    }

    #[test]
    fn routing_config_defaults() {
        let cfg = RoutingConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.method, RoutingMode::Probabilistic);
        assert!((cfg.bayesian_alpha - 1.0).abs() < f64::EPSILON);
        assert!((cfg.confidence_threshold - 0.3).abs() < f64::EPSILON);
        assert!((cfg.recency_decay_half_life_hours - 168.0).abs() < f64::EPSILON);
        assert!((cfg.confidence_ema_alpha - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn debate_config_defaults() {
        let cfg = DebateConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.default_max_rounds, 3);
        assert_eq!(cfg.min_evidence_refs, 1);
        assert!(cfg.role_rotation);
        assert_eq!(
            cfg.verdict_required_sections,
            vec![
                "consensus_points".to_string(),
                "unresolved_tensions".to_string(),
                "recommended_action".to_string(),
            ]
        );
    }

    #[test]
    fn critique_config_defaults() {
        let cfg = CritiqueConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.mode, CritiqueMode::Disabled);
        assert!(cfg.guard_suspicious_tool_calls_only);
    }

    #[test]
    fn agent_config_deserializes_debate_without_disturbing_other_defaults() {
        let json = serde_json::json!({
            "debate": {
                "enabled": true,
                "default_max_rounds": 4,
                "min_evidence_refs": 2,
                "role_rotation": false,
                "verdict_required_sections": ["consensus_points", "recommended_action"]
            }
        })
        .to_string();

        let cfg: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(cfg.debate.enabled);
        assert_eq!(cfg.debate.default_max_rounds, 4);
        assert_eq!(cfg.debate.min_evidence_refs, 2);
        assert!(!cfg.debate.role_rotation);
        assert_eq!(
            cfg.debate.verdict_required_sections,
            vec!["consensus_points".to_string(), "recommended_action".to_string()]
        );
        assert!(cfg.skill_recommendation.enabled);
    }

    #[test]
    fn agent_config_deserializes_critique_without_disturbing_other_defaults() {
        let json = serde_json::json!({
            "critique": {
                "enabled": true,
                "mode": "deterministic",
                "guard_suspicious_tool_calls_only": false
            }
        })
        .to_string();

        let cfg: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(cfg.critique.enabled);
        assert_eq!(cfg.critique.mode, CritiqueMode::Deterministic);
        assert!(!cfg.critique.guard_suspicious_tool_calls_only);
        assert!(cfg.skill_recommendation.enabled);
    }

    #[test]
    fn agent_config_deserializes_routing_without_disturbing_other_defaults() {
        let json = serde_json::json!({
            "routing": {
                "enabled": false,
                "method": "deterministic",
                "bayesian_alpha": 2.0,
                "confidence_threshold": 0.55,
                "recency_decay_half_life_hours": 24.0,
                "confidence_ema_alpha": 0.45
            }
        })
        .to_string();

        let cfg: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(!cfg.routing.enabled);
        assert_eq!(cfg.routing.method, RoutingMode::Deterministic);
        assert!((cfg.routing.bayesian_alpha - 2.0).abs() < f64::EPSILON);
        assert!((cfg.routing.confidence_threshold - 0.55).abs() < f64::EPSILON);
        assert!((cfg.routing.recency_decay_half_life_hours - 24.0).abs() < f64::EPSILON);
        assert!((cfg.routing.confidence_ema_alpha - 0.45).abs() < f64::EPSILON);
        assert!(cfg.skill_recommendation.enabled);
    }

    #[test]
    fn agent_config_defaults_include_routing() {
        let cfg: AgentConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.routing.enabled);
        assert_eq!(cfg.routing.method, RoutingMode::Probabilistic);
        assert!((cfg.routing.confidence_threshold - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn agent_config_deserializes_legacy_skill_discovery_without_skill_recommendation() {
        let json = serde_json::json!({
            "skill_discovery": {
                "min_tool_count": 13,
                "min_replan_count": 2,
                "min_quality_score": 0.91,
                "novelty_similarity_threshold": 0.42
            }
        })
        .to_string();

        let cfg: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.skill_discovery.min_tool_count, 13);
        assert_eq!(cfg.skill_discovery.min_replan_count, 2);
        assert!((cfg.skill_discovery.min_quality_score - 0.91).abs() < f64::EPSILON);
        assert!((cfg.skill_discovery.novelty_similarity_threshold - 0.42).abs() < f64::EPSILON);
        assert!(cfg.skill_recommendation.enabled);
        assert_eq!(cfg.skill_recommendation.discovery_backend, "mesh");
        assert!(cfg.skill_recommendation.require_read_on_strong_match);
        assert!((cfg.skill_recommendation.strong_match_threshold - 0.85).abs() < f64::EPSILON);
        assert!((cfg.skill_recommendation.weak_match_threshold - 0.60).abs() < f64::EPSILON);
        assert!((cfg.skill_recommendation.novelty_distance_weight - 0.05).abs() < f64::EPSILON);
        assert!(cfg.skill_recommendation.background_community_search);
        assert_eq!(cfg.skill_recommendation.community_preapprove_timeout_secs, 30);
        assert_eq!(cfg.skill_recommendation.suggest_global_enable_after_approvals, 3);
        assert!(cfg.skill_recommendation.llm_normalize_on_no_match);
        assert!(cfg.skill_recommendation.llm_semantic_search_on_no_match);
        assert_eq!(cfg.skill_recommendation.llm_semantic_search_max_skills, 64);
    }

    #[test]
    fn agent_config_deserializes_skill_recommendation_without_disturbing_skill_discovery() {
        let json = serde_json::json!({
            "skill_discovery": {
                "min_tool_count": 21,
                "min_replan_count": 4,
                "min_quality_score": 0.95,
                "novelty_similarity_threshold": 0.33
            },
            "skill_recommendation": {
                "enabled": false,
                "discovery_backend": "mesh",
                "require_read_on_strong_match": false,
                "strong_match_threshold": 0.97,
                "weak_match_threshold": 0.51,
                "novelty_distance_weight": 0.22,
                "background_community_search": false,
                "community_preapprove_timeout_secs": 45,
                "suggest_global_enable_after_approvals": 8
            }
        })
        .to_string();

        let cfg: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.skill_discovery.min_tool_count, 21);
        assert_eq!(cfg.skill_discovery.min_replan_count, 4);
        assert!((cfg.skill_discovery.min_quality_score - 0.95).abs() < f64::EPSILON);
        assert!((cfg.skill_discovery.novelty_similarity_threshold - 0.33).abs() < f64::EPSILON);
        assert!(!cfg.skill_recommendation.enabled);
        assert_eq!(cfg.skill_recommendation.discovery_backend, "mesh");
        assert!(!cfg.skill_recommendation.require_read_on_strong_match);
        assert!((cfg.skill_recommendation.strong_match_threshold - 0.97).abs() < f64::EPSILON);
        assert!((cfg.skill_recommendation.weak_match_threshold - 0.51).abs() < f64::EPSILON);
        assert!((cfg.skill_recommendation.novelty_distance_weight - 0.22).abs() < f64::EPSILON);
        assert!(!cfg.skill_recommendation.background_community_search);
        assert_eq!(cfg.skill_recommendation.community_preapprove_timeout_secs, 45);
        assert_eq!(cfg.skill_recommendation.suggest_global_enable_after_approvals, 8);
    }

    #[test]
    fn consolidation_result_has_skill_fields() {
        let result = ConsolidationResult::default();
        assert_eq!(result.skill_candidates_flagged, 0);
        assert_eq!(result.skills_drafted, 0);
        assert_eq!(result.skills_tested, 0);
        assert_eq!(result.skills_promoted, 0);
    }

    #[test]
    fn heartbeat_check_type_skill_lifecycle_serializes() {
        let check = HeartbeatCheckType::SkillLifecycle;
        assert_eq!(
            serde_json::to_string(&check).unwrap(),
            r#""skill_lifecycle""#
        );
        let roundtripped: HeartbeatCheckType =
            serde_json::from_str(r#""skill_lifecycle""#).unwrap();
        assert_eq!(roundtripped, HeartbeatCheckType::SkillLifecycle);
    }

    #[test]
    fn heartbeat_check_type_plugin_auth_serializes() {
        let check = HeartbeatCheckType::PluginAuth;
        assert_eq!(serde_json::to_string(&check).unwrap(), r#""plugin_auth""#);
        let roundtripped: HeartbeatCheckType =
            serde_json::from_str(r#""plugin_auth""#).unwrap();
        assert_eq!(roundtripped, HeartbeatCheckType::PluginAuth);
    }

    #[test]
    fn skill_maturity_status_as_str() {
        assert_eq!(SkillMaturityStatus::Draft.as_str(), "draft");
        assert_eq!(SkillMaturityStatus::Testing.as_str(), "testing");
        assert_eq!(SkillMaturityStatus::Active.as_str(), "active");
        assert_eq!(SkillMaturityStatus::Proven.as_str(), "proven");
        assert_eq!(
            SkillMaturityStatus::PromotedToCanonical.as_str(),
            "promoted_to_canonical"
        );
    }

    #[test]
    fn skill_maturity_status_from_status_str() {
        assert_eq!(
            SkillMaturityStatus::from_status_str("draft"),
            Some(SkillMaturityStatus::Draft)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("testing"),
            Some(SkillMaturityStatus::Testing)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("active"),
            Some(SkillMaturityStatus::Active)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("proven"),
            Some(SkillMaturityStatus::Proven)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("promoted_to_canonical"),
            Some(SkillMaturityStatus::PromotedToCanonical)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("promoted-to-canonical"),
            Some(SkillMaturityStatus::PromotedToCanonical)
        );
        assert_eq!(SkillMaturityStatus::from_status_str("bogus"), None);
    }

    #[test]
    fn agent_config_has_skill_discovery_and_promotion() {
        let cfg: AgentConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.skill_discovery.min_tool_count, 8);
        assert_eq!(cfg.skill_promotion.testing_to_active, 3);
    }

    #[test]
    fn github_copilot_provider_exposes_static_catalog_models() {
        let provider =
            get_provider_definition(PROVIDER_ID_GITHUB_COPILOT).expect("copilot provider");
        assert!(!provider.models.is_empty());
        assert_eq!(provider.default_model, "gpt-4.1");
        assert_eq!(provider.default_transport, ApiTransport::Responses);
        assert!(provider
            .supported_transports
            .contains(&ApiTransport::Responses));
        assert!(provider
            .supported_transports
            .contains(&ApiTransport::ChatCompletions));
        assert!(provider
            .supported_transports
            .contains(&ApiTransport::AnthropicMessages));
        assert!(provider.models.iter().any(|model| model.id == "gpt-5.4"));
        assert!(provider
            .models
            .iter()
            .any(|model| model.id == "claude-opus-4.6"));
        assert!(provider.models.iter().any(|model| model.id == "goldeneye"));
    }

    #[test]
    fn arcee_provider_exposes_openai_compatible_defaults() {
        let provider = get_provider_definition(PROVIDER_ID_ARCEE).expect("arcee provider");
        assert_eq!(provider.default_base_url, "https://api.arcee.ai/api/v1");
        assert_eq!(provider.default_model, "trinity-large-thinking");
        assert_eq!(provider.api_type, ApiType::OpenAI);
        assert_eq!(provider.auth_method, AuthMethod::Bearer);
        assert!(provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(provider.models.len(), 1);
        assert_eq!(provider.models[0].id, "trinity-large-thinking");
        assert_eq!(provider.models[0].context_window, 256000);
        assert_eq!(
            get_provider_api_type(
                PROVIDER_ID_ARCEE,
                "trinity-large-thinking",
                "https://api.arcee.ai/api/v1"
            ),
            ApiType::OpenAI
        );
    }

    #[test]
    fn nvidia_provider_exposes_fetchable_openai_defaults() {
        let provider = get_provider_definition(PROVIDER_ID_NVIDIA).expect("nvidia provider");
        assert_eq!(provider.default_base_url, "https://integrate.api.nvidia.com/v1");
        assert_eq!(provider.default_model, "minimaxai/minimax-m2.7");
        assert_eq!(provider.api_type, ApiType::OpenAI);
        assert_eq!(provider.auth_method, AuthMethod::Bearer);
        assert!(provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(provider.models.len(), 1);
        assert_eq!(provider.models[0].id, "minimaxai/minimax-m2.7");
        assert_eq!(provider.models[0].context_window, 205000);
        assert_eq!(
            get_provider_api_type(
                PROVIDER_ID_NVIDIA,
                "minimaxai/minimax-m2.7",
                "https://integrate.api.nvidia.com/v1"
            ),
            ApiType::OpenAI
        );
    }

    #[test]
    fn chutes_provider_exposes_fetchable_openai_defaults() {
        let provider = get_provider_definition(PROVIDER_ID_CHUTES).expect("chutes provider");
        assert_eq!(provider.default_base_url, "https://llm.chutes.ai/v1");
        assert_eq!(provider.default_model, "deepseek-ai/DeepSeek-R1");
        assert_eq!(provider.api_type, ApiType::OpenAI);
        assert_eq!(provider.auth_method, AuthMethod::Bearer);
        assert!(provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(provider.models.len(), 1);
        assert_eq!(provider.models[0].id, "deepseek-ai/DeepSeek-R1");
        assert_eq!(provider.models[0].context_window, 128_000);
        assert_eq!(
            get_provider_api_type(
                PROVIDER_ID_CHUTES,
                "deepseek-ai/DeepSeek-R1",
                "https://llm.chutes.ai/v1"
            ),
            ApiType::OpenAI
        );
    }

    #[test]
    fn deepseek_provider_exposes_fetchable_openai_defaults() {
        let provider = get_provider_definition(PROVIDER_ID_DEEPSEEK).expect("deepseek provider");
        assert_eq!(provider.default_base_url, "https://api.deepseek.com");
        assert_eq!(provider.default_model, "deepseek-v4-pro");
        assert_eq!(provider.api_type, ApiType::OpenAI);
        assert_eq!(provider.auth_method, AuthMethod::Bearer);
        assert!(provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(provider.models.len(), 2);
        assert_eq!(provider.models[0].id, "deepseek-v4-pro");
        assert_eq!(provider.models[0].context_window, 1_048_576);
        assert_eq!(provider.models[1].id, "deepseek-v4-flash");
        assert_eq!(provider.models[1].context_window, 1_048_576);
        assert_eq!(
            get_provider_api_type(
                PROVIDER_ID_DEEPSEEK,
                "deepseek-v4-pro",
                "https://api.deepseek.com"
            ),
            ApiType::OpenAI
        );
    }

    #[test]
    fn xai_provider_exposes_fetchable_responses_defaults() {
        let provider = get_provider_definition(PROVIDER_ID_XAI).expect("xai provider");
        assert_eq!(provider.default_base_url, "https://api.x.ai/v1");
        assert_eq!(provider.default_model, "grok-4");
        assert_eq!(provider.api_type, ApiType::OpenAI);
        assert_eq!(provider.auth_method, AuthMethod::Bearer);
        assert!(provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::Responses);
        assert!(provider
            .supported_transports
            .contains(&ApiTransport::Responses));
        assert!(provider
            .supported_transports
            .contains(&ApiTransport::ChatCompletions));
        assert_eq!(provider.models.len(), 2);
        assert_eq!(provider.models[0].id, "grok-4");
        assert_eq!(provider.models[0].context_window, 262_144);
        assert_eq!(provider.models[1].id, "grok-code-fast-1");
        assert_eq!(provider.models[1].context_window, 173_000);
        assert_eq!(
            get_provider_api_type(PROVIDER_ID_XAI, "grok-4", "https://api.x.ai/v1"),
            ApiType::OpenAI
        );
    }

    #[test]
    fn xiaomi_mimo_token_plan_exposes_static_openai_defaults() {
        let provider =
            get_provider_definition("xiaomi-mimo-token-plan").expect("xiaomi mimo provider");
        assert_eq!(provider.default_base_url, "https://api.xiaomimimo.com/v1");
        assert_eq!(provider.default_model, "mimo-v2-pro");
        assert_eq!(provider.api_type, ApiType::OpenAI);
        assert_eq!(provider.auth_method, AuthMethod::Bearer);
        assert!(!provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(provider.models.len(), 7);
        assert_eq!(provider.models[0].id, "mimo-v2-pro");
        assert_eq!(provider.models[0].context_window, 1_000_000);
        assert_eq!(provider.models[1].id, "mimo-v2-omni");
        assert_eq!(provider.models[1].context_window, 256_000);
        assert_eq!(provider.models[2].id, "mimo-v2.5-pro");
        assert_eq!(provider.models[2].context_window, 1_000_000);
        assert_eq!(provider.models[3].id, "mimo-v2.5");
        assert_eq!(provider.models[3].context_window, 1_000_000);
        assert_eq!(provider.models[4].id, "mimo-v2.5-tts");
        assert_eq!(provider.models[5].id, "mimo-v2.5-tts-voiceclone");
        assert_eq!(provider.models[6].id, "mimo-v2.5-tts-voicedesign");
        assert_eq!(
            get_provider_api_type(
                "xiaomi-mimo-token-plan",
                "mimo-v2-pro",
                "https://api.xiaomimimo.com/v1"
            ),
            ApiType::OpenAI
        );
    }

    #[test]
    fn nous_portal_exposes_fetchable_openai_defaults() {
        let provider = get_provider_definition("nous-portal").expect("nous portal provider");
        assert_eq!(
            provider.default_base_url,
            "https://inference-api.nousresearch.com/v1"
        );
        assert_eq!(provider.default_model, "nousresearch/hermes-4-70b");
        assert_eq!(provider.api_type, ApiType::OpenAI);
        assert_eq!(provider.auth_method, AuthMethod::Bearer);
        assert!(provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(provider.models.len(), 4);
        assert_eq!(provider.models[0].id, "nousresearch/hermes-4-70b");
        assert_eq!(provider.models[0].context_window, 131_072);
        assert_eq!(provider.models[1].id, "nousresearch/hermes-4-405b");
        assert_eq!(provider.models[1].context_window, 131_072);
        assert_eq!(
            get_provider_api_type(
                "nous-portal",
                "nousresearch/hermes-4-70b",
                "https://inference-api.nousresearch.com/v1"
            ),
            ApiType::OpenAI
        );
    }

    #[test]
    fn anthropic_provider_exposes_static_anthropic_defaults() {
        let provider = get_provider_definition(PROVIDER_ID_ANTHROPIC).expect("anthropic provider");
        assert_eq!(provider.default_base_url, "https://api.anthropic.com");
        assert_eq!(provider.default_model, "claude-opus-4-7");
        assert_eq!(provider.api_type, ApiType::Anthropic);
        assert_eq!(provider.auth_method, AuthMethod::XApiKey);
        assert!(!provider.supports_model_fetch);
        assert_eq!(provider.default_transport, ApiTransport::ChatCompletions);
        assert_eq!(provider.models.len(), 13);
        assert_eq!(provider.models[0].id, "claude-opus-4-7");
        assert_eq!(provider.models[0].context_window, 1_000_000);
        assert_eq!(provider.models[5].id, "claude-sonnet-4-6");
        assert_eq!(provider.models[5].context_window, 1_000_000);
        assert_eq!(provider.models[9].id, "claude-haiku-4-5-20251001");
        assert_eq!(provider.models[9].context_window, 200_000);
        assert_eq!(
            get_provider_api_type(
                PROVIDER_ID_ANTHROPIC,
                "claude-opus-4-7",
                "https://api.anthropic.com"
            ),
            ApiType::Anthropic
        );
    }

    #[test]
    fn goal_dossier_serializes_into_goal_run_state() {
        let goal_run = GoalRun {
            id: "goal-run-1".to_string(),
            title: "Ship dossier projection".to_string(),
            goal: "Add dossier support to the daemon goal state".to_string(),
            client_request_id: None,
            status: GoalRunStatus::Paused,
            priority: TaskPriority::Normal,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_100,
            started_at: Some(1_700_000_010),
            completed_at: None,
            thread_id: Some("thread-1".to_string()),
            root_thread_id: Some("thread-1".to_string()),
            active_thread_id: Some("thread-1".to_string()),
            execution_thread_ids: vec!["thread-1".to_string()],
            session_id: Some("session-1".to_string()),
            current_step_index: 0,
            current_step_title: Some("Draft dossier types".to_string()),
            current_step_kind: Some(GoalRunStepKind::Reason),
            launch_assignment_snapshot: Vec::new(),
            runtime_assignment_list: Vec::new(),
            planner_owner_profile: None,
            current_step_owner_profile: None,
            replan_count: 0,
            max_replans: 3,
            plan_summary: Some("Design a compact dossier layer".to_string()),
            reflection_summary: None,
            memory_updates: vec![],
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            child_task_ids: vec![],
            child_task_count: 0,
            approval_count: 0,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: None,
            duration_ms: None,
            steps: vec![],
            events: vec![],
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            autonomy_level: AutonomyLevel::default(),
            authorship_tag: None,
            dossier: Some(GoalRunDossier {
                units: vec![GoalDeliveryUnit {
                    id: "delivery-unit-1".to_string(),
                    title: "Verify dossier plumbing".to_string(),
                    status: GoalProjectionState::Pending,
                    execution_binding: GoalRoleBinding::Builtin("swarog".to_string()),
                    verification_binding: GoalRoleBinding::Subagent(
                        "android-verifier".to_string(),
                    ),
                    ..Default::default()
                }],
                latest_resume_decision: Some(GoalResumeDecision {
                    action: GoalResumeAction::Advance,
                    reason_code: "proof_complete".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            stopped_reason: Some("manual stop after proof capture".to_string()),
        };

        let json = serde_json::to_string(&goal_run).unwrap();
        assert!(json.contains("\"dossier\""));
        assert!(json.contains("android-verifier"));
        assert!(json.contains("\"stopped_reason\":\"manual stop after proof capture\""));
    }

    #[test]
    fn goal_dossier_deserializes_sparse_payloads_with_defaults() {
        let dossier: GoalRunDossier = serde_json::from_str(
            r#"{
                "units": [
                    {
                        "status": "completed",
                        "proof_checks": [{}],
                        "evidence": [{}],
                        "report": {
                            "proof_checks": [{}],
                            "evidence": [{}]
                        }
                    }
                ],
                "latest_resume_decision": {
                    "action": "pause"
                }
            }"#,
        )
        .expect("sparse dossier payload should deserialize");

        assert_eq!(dossier.units.len(), 1);
        assert_eq!(dossier.units[0].id, "");
        assert_eq!(dossier.units[0].title, "");
        assert_eq!(dossier.units[0].proof_checks[0].id, "");
        assert_eq!(dossier.units[0].evidence[0].title, "");
        assert_eq!(
            dossier
                .latest_resume_decision
                .expect("resume decision should exist")
                .reason_code,
            ""
        );
    }
