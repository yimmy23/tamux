use amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT;

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
        assert!(provider.models.iter().any(|model| model.id == "gpt-5.4"));
        assert!(provider
            .models
            .iter()
            .any(|model| model.id == "claude-opus-4.6"));
        assert!(provider.models.iter().any(|model| model.id == "goldeneye"));
    }
