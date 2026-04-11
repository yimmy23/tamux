use super::*;

fn map_skill_variant_list_response(resp: DaemonMessage) -> Result<Value> {
    match resp {
        DaemonMessage::SkillListResult {
            variants,
            next_cursor,
        } => Ok(serde_json::json!({
            "variants": variants,
            "next_cursor": next_cursor,
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

fn map_skill_variant_inspect_response(identifier: &str, resp: DaemonMessage) -> Result<Value> {
    match resp {
        DaemonMessage::SkillInspectResult { variant, content } => Ok(serde_json::json!({
            "identifier": identifier,
            "variant": variant,
            "content": content,
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

fn map_skill_discovery_response(resp: DaemonMessage) -> Result<Value> {
    match resp {
        DaemonMessage::SkillDiscoverResult { result_json } => serde_json::from_str(&result_json)
            .map_err(|error| anyhow::anyhow!("invalid daemon skill discovery payload: {error}")),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

fn parse_skill_discovery_event(resp: DaemonMessage) -> Option<Result<Value>> {
    match resp {
        DaemonMessage::CwdChanged { .. }
        | DaemonMessage::Output { .. }
        | DaemonMessage::CommandStarted { .. }
        | DaemonMessage::CommandFinished { .. } => None,
        other => Some(map_skill_discovery_response(other)),
    }
}

fn map_audit_query_response(resp: DaemonMessage) -> Result<Value> {
    match resp {
        DaemonMessage::AuditList { entries_json } => Ok(serde_json::json!({
            "entries": serde_json::from_str::<Value>(&entries_json).unwrap_or(Value::Array(Vec::new())),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

fn map_question_response(resp: DaemonMessage) -> Result<Value> {
    match resp {
        DaemonMessage::AgentQuestionAnswered { answer, .. } => Ok(Value::String(answer)),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_list_skill_variants(args: &Value) -> Result<Value> {
    let status = args
        .get("status")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 200) as usize)
        .unwrap_or(25);
    let cursor = args
        .get("cursor")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let resp = daemon_roundtrip(ClientMessage::SkillList {
        status,
        limit,
        cursor,
    })
    .await?;
    map_skill_variant_list_response(resp)
}

pub(super) async fn tool_inspect_skill_variant(args: &Value) -> Result<Value> {
    let identifier = args
        .get("identifier")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: identifier"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::SkillInspect {
        identifier: identifier.clone(),
    })
    .await?;
    map_skill_variant_inspect_response(&identifier, resp)
}

pub(super) async fn tool_discover_skills(args: &Value) -> Result<Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: query"))?
        .to_string();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 20) as usize)
        .unwrap_or(3);
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|value| {
            amux_protocol::SessionId::parse_str(value)
                .map_err(|error| anyhow::anyhow!("invalid session_id `{value}`: {error}"))
        })
        .transpose()?;
    let cursor = args
        .get("cursor")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    super::daemon::daemon_roundtrip_until(
        ClientMessage::SkillDiscover {
            query,
            session_id,
            limit,
            cursor,
        },
        parse_skill_discovery_event,
    )
    .await
}

pub(super) async fn tool_ask_questions(args: &Value) -> Result<Value> {
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: content"))?
        .to_string();
    let options = args
        .get("options")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: options"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|option| !option.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| anyhow::anyhow!("options must contain only non-empty strings"))
        })
        .collect::<Result<Vec<_>>>()?;
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    let resp = daemon_roundtrip(ClientMessage::AgentAskQuestion {
        content,
        options,
        session_id,
    })
    .await?;
    map_question_response(resp)
}

pub(super) async fn tool_list_goal_runs() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentListGoalRuns).await?;

    match resp {
        DaemonMessage::AgentGoalRunList { goal_runs_json } => Ok(serde_json::json!({
            "goal_runs": serde_json::from_str::<Value>(&goal_runs_json).unwrap_or(Value::Array(Vec::new())),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_get_goal_run(args: &Value) -> Result<Value> {
    let goal_run_id = args
        .get("goal_run_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: goal_run_id"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::AgentGetGoalRun {
        goal_run_id: goal_run_id.clone(),
    })
    .await?;

    match resp {
        DaemonMessage::AgentGoalRunDetail { goal_run_json } => Ok(serde_json::json!({
            "goal_run_id": goal_run_id,
            "goal_run": serde_json::from_str::<Value>(&goal_run_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_control_goal_run(args: &Value) -> Result<Value> {
    let goal_run_id = args
        .get("goal_run_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: goal_run_id"))?
        .to_string();
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: action"))?
        .to_string();
    let step_index = args
        .get("step_index")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::AgentControlGoalRun {
        goal_run_id: goal_run_id.clone(),
        action: action.clone(),
        step_index,
    })
    .await?;

    match resp {
        DaemonMessage::AgentGoalRunControlled { goal_run_id, ok } => Ok(serde_json::json!({
            "goal_run_id": goal_run_id,
            "action": action,
            "step_index": step_index,
            "ok": ok,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_get_operator_model() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentGetOperatorModel).await?;

    match resp {
        DaemonMessage::AgentOperatorModel { model_json } => Ok(serde_json::json!({
            "operator_model": serde_json::from_str::<Value>(&model_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_reset_operator_model() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentResetOperatorModel).await?;

    match resp {
        DaemonMessage::AgentOperatorModelReset { ok } => Ok(serde_json::json!({
            "ok": ok,
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_query_audits(args: &Value) -> Result<Value> {
    let action_types = args
        .get("action_types")
        .and_then(|v| v.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty());
    let since = args.get("since").and_then(|v| v.as_u64());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 500) as usize);

    let resp = daemon_roundtrip(ClientMessage::AuditQuery {
        action_types,
        since,
        limit,
    })
    .await?;

    map_audit_query_response(resp)
}

pub(super) async fn tool_get_causal_trace_report(args: &Value) -> Result<Value> {
    let option_type = args
        .get("option_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: option_type"))?
        .to_string();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 200) as u32);

    let resp = daemon_roundtrip(ClientMessage::AgentGetCausalTraceReport {
        option_type: option_type.clone(),
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::AgentCausalTraceReport { report_json } => Ok(serde_json::json!({
            "option_type": option_type,
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_get_counterfactual_report(args: &Value) -> Result<Value> {
    let option_type = args
        .get("option_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: option_type"))?
        .to_string();
    let command_family = args
        .get("command_family")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: command_family"))?
        .to_string();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 200) as u32);

    let resp = daemon_roundtrip(ClientMessage::AgentGetCounterfactualReport {
        option_type: option_type.clone(),
        command_family: command_family.clone(),
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::AgentCounterfactualReport { report_json } => Ok(serde_json::json!({
            "option_type": option_type,
            "command_family": command_family,
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_get_memory_provenance_report(args: &Value) -> Result<Value> {
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 200) as u32);

    let resp = daemon_roundtrip(ClientMessage::AgentGetMemoryProvenanceReport {
        target: target.clone(),
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::AgentMemoryProvenanceReport { report_json } => Ok(serde_json::json!({
            "target": target,
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_get_provenance_report(args: &Value) -> Result<Value> {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 500) as u32);
    let resp = daemon_roundtrip(ClientMessage::AgentGetProvenanceReport { limit }).await?;
    match resp {
        DaemonMessage::AgentProvenanceReport { report_json } => Ok(serde_json::json!({
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_generate_soc2_artifact(args: &Value) -> Result<Value> {
    let period_days = args
        .get("period_days")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 365) as u32);
    let resp = daemon_roundtrip(ClientMessage::AgentGenerateSoc2Artifact { period_days }).await?;
    match resp {
        DaemonMessage::AgentSoc2Artifact { artifact_path } => Ok(serde_json::json!({
            "artifact_path": artifact_path,
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_get_collaboration_sessions(args: &Value) -> Result<Value> {
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let resp = daemon_roundtrip(ClientMessage::AgentGetCollaborationSessions {
        parent_task_id: parent_task_id.clone(),
    })
    .await?;
    match resp {
        DaemonMessage::AgentCollaborationSessions { sessions_json } => Ok(serde_json::json!({
            "parent_task_id": parent_task_id,
            "sessions": serde_json::from_str::<Value>(&sessions_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_list_generated_tools() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentListGeneratedTools).await?;
    match resp {
        DaemonMessage::AgentGeneratedTools { tools_json } => Ok(serde_json::json!({
            "tools": serde_json::from_str::<Value>(&tools_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_synthesize_tool(args: &Value) -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentSynthesizeTool {
        request_json: serde_json::to_string(args)?,
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            operation_id: _,
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": serde_json::from_str::<Value>(&result_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_run_generated_tool(args: &Value) -> Result<Value> {
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: tool_name"))?
        .to_string();
    let tool_args = args
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let resp = daemon_roundtrip(ClientMessage::AgentRunGeneratedTool {
        tool_name: tool_name.clone(),
        args_json: serde_json::to_string(&tool_args)?,
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            operation_id: _,
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": result_json,
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_promote_generated_tool(args: &Value) -> Result<Value> {
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: tool_name"))?
        .to_string();
    let resp = daemon_roundtrip(ClientMessage::AgentPromoteGeneratedTool {
        tool_name: tool_name.clone(),
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            operation_id: _,
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": serde_json::from_str::<Value>(&result_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_activate_generated_tool(args: &Value) -> Result<Value> {
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: tool_name"))?
        .to_string();
    let resp = daemon_roundtrip(ClientMessage::AgentActivateGeneratedTool {
        tool_name: tool_name.clone(),
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            operation_id: _,
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": serde_json::from_str::<Value>(&result_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amux_protocol::SkillVariantPublic;

    fn sample_variant() -> SkillVariantPublic {
        SkillVariantPublic {
            variant_id: "var-1".to_string(),
            skill_name: "build-pipeline".to_string(),
            variant_name: "frontend".to_string(),
            relative_path: "generated/build-pipeline--frontend.md".to_string(),
            status: "active".to_string(),
            use_count: 5,
            success_count: 4,
            failure_count: 1,
            context_tags: vec!["frontend".to_string(), "rust".to_string()],
            created_at: 100,
            updated_at: 200,
        }
    }

    #[test]
    fn map_skill_variant_list_response_returns_variants_payload() {
        let value = map_skill_variant_list_response(DaemonMessage::SkillListResult {
            variants: vec![sample_variant()],
            next_cursor: Some("cursor:variant-2".to_string()),
        })
        .expect("list response should map");

        assert_eq!(value["variants"][0]["skill_name"], "build-pipeline");
        assert_eq!(value["variants"][0]["variant_name"], "frontend");
        assert_eq!(value["next_cursor"], "cursor:variant-2");
    }

    #[test]
    fn map_skill_variant_inspect_response_preserves_content_note() {
        let value = map_skill_variant_inspect_response(
            "var-1",
            DaemonMessage::SkillInspectResult {
                variant: Some(sample_variant()),
                content: Some(
                    "## Lifecycle Inspection\n- Status rationale: active branch\n\n# Skill"
                        .to_string(),
                ),
            },
        )
        .expect("inspect response should map");

        assert_eq!(value["identifier"], "var-1");
        assert_eq!(value["variant"]["variant_id"], "var-1");
        assert!(value["content"]
            .as_str()
            .is_some_and(|content| content.contains("Lifecycle Inspection")));
    }

    #[test]
    fn map_audit_query_response_returns_entries_payload() {
        let value = map_audit_query_response(DaemonMessage::AuditList {
            entries_json: serde_json::json!([
                {
                    "id": 7,
                    "action_type": "tool",
                    "summary": "Executed managed command",
                    "timestamp": 1234
                }
            ])
            .to_string(),
        })
        .expect("audit response should map");

        assert_eq!(value["entries"][0]["action_type"], "tool");
        assert_eq!(value["entries"][0]["summary"], "Executed managed command");
    }

    #[test]
    fn parse_skill_discovery_event_ignores_unsolicited_frames() {
        let session_id =
            uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("valid test uuid");

        assert!(parse_skill_discovery_event(DaemonMessage::CwdChanged {
            id: session_id,
            cwd: "/workspace/repo".to_string(),
        })
        .is_none());
        assert!(parse_skill_discovery_event(DaemonMessage::Output {
            id: session_id,
            data: b"log".to_vec(),
        })
        .is_none());
        assert!(parse_skill_discovery_event(DaemonMessage::CommandStarted {
            id: session_id,
            command: "cargo test".to_string(),
        })
        .is_none());
        assert!(parse_skill_discovery_event(DaemonMessage::CommandFinished {
            id: session_id,
            exit_code: Some(0),
        })
        .is_none());
    }

    #[test]
    fn parse_skill_discovery_event_maps_result_payload() {
        let value = parse_skill_discovery_event(DaemonMessage::SkillDiscoverResult {
            result_json: serde_json::json!({
                "query": "debug panic",
                "normalized_intent": "debug panic root cause",
                "required": true,
                "confidence_tier": "strong",
                "recommended_action": "read_skill systematic-debugging",
                "requires_approval": false,
                "mesh_state": "fresh",
                "rationale": ["matched debug intent"],
                "capability_family": ["development", "debugging"],
                "explicit_rationale_required": false,
                "workspace_tags": ["rust"],
                "candidates": [{
                    "skill_name": "systematic-debugging",
                    "matched_intents": ["debug panic root cause"],
                    "matched_trigger_phrases": ["panic"],
                    "risk_level": "low",
                    "trust_tier": "trusted_builtin",
                    "source_kind": "builtin",
                    "recommended_action": "read_skill systematic-debugging"
                }],
                "next_cursor": "cursor:skill-2"
            })
            .to_string(),
        })
        .expect("result frame should terminate")
        .expect("payload should parse");

        assert_eq!(value["query"], "debug panic");
        assert_eq!(value["normalized_intent"], "debug panic root cause");
        assert_eq!(value["confidence_tier"], "strong");
        assert_eq!(value["mesh_state"], "fresh");
        assert_eq!(value["capability_family"][0], "development");
        assert_eq!(value["candidates"][0]["trust_tier"], "trusted_builtin");
        assert_eq!(value["next_cursor"], "cursor:skill-2");
    }
}
