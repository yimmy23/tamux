use super::*;

fn map_skill_variant_list_response(resp: DaemonMessage) -> Result<Value> {
    match resp {
        DaemonMessage::SkillListResult { variants } => Ok(serde_json::json!({
            "variants": variants,
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

    let resp = daemon_roundtrip(ClientMessage::SkillList { status, limit }).await?;
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
        })
        .expect("list response should map");

        assert_eq!(value["variants"][0]["skill_name"], "build-pipeline");
        assert_eq!(value["variants"][0]["variant_name"], "frontend");
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
        assert!(
            value["content"]
                .as_str()
                .is_some_and(|content| content.contains("Lifecycle Inspection"))
        );
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
}
