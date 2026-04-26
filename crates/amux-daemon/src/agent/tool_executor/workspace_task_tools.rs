use amux_protocol::{
    WorkspaceActor, WorkspaceCompletionSubmission, WorkspaceOperator, WorkspacePriority,
    WorkspaceReviewSubmission, WorkspaceReviewVerdict, WorkspaceTaskCreate, WorkspaceTaskMove,
    WorkspaceTaskStatus, WorkspaceTaskType,
};

const WORKSPACE_READ_TOOLS: [&str; 4] = [
    "workspace_get_settings",
    "workspace_list_tasks",
    "workspace_get_task",
    "workspace_list_notices",
];

const WORKSPACE_MUTATION_TOOLS: [&str; 9] = [
    "workspace_set_operator",
    "workspace_create_task",
    "workspace_update_task",
    "workspace_move_task",
    "workspace_run_task",
    "workspace_pause_task",
    "workspace_stop_task",
    "workspace_delete_task",
    "workspace_submit_review",
];

const WORKSPACE_ASSIGNEE_TOOLS: [&str; 1] = ["workspace_submit_completion"];

fn add_workspace_task_tools(tools: &mut Vec<ToolDefinition>) {
    tools.push(tool_def("workspace_get_settings", "Read workspace board settings and operator mode.", serde_json::json!({
        "type": "object",
        "properties": { "workspace_id": { "type": "string", "description": "Workspace id, defaults to main" } }
    })));
    tools.push(tool_def("workspace_list_tasks", "List workspace tasks across todo, in-progress, in-review, and done columns.", serde_json::json!({
        "type": "object",
        "properties": {
            "workspace_id": { "type": "string", "description": "Workspace id, defaults to main" },
            "include_deleted": { "type": "boolean", "description": "Include soft-deleted tasks" }
        }
    })));
    tools.push(tool_def("workspace_get_task", "Read a single workspace task by id.", serde_json::json!({
        "type": "object",
        "properties": { "task_id": { "type": "string" } },
        "required": ["task_id"]
    })));
    tools.push(tool_def("workspace_list_notices", "Read workspace notices, optionally scoped to one task.", serde_json::json!({
        "type": "object",
        "properties": {
            "workspace_id": { "type": "string", "description": "Workspace id, defaults to main" },
            "task_id": { "type": "string" }
        }
    })));
    if current_agent_scope_id() != MAIN_AGENT_ID {
        add_workspace_submit_review_tool(tools);
        add_workspace_submit_completion_tool(tools);
        return;
    }
    tools.push(tool_def("workspace_set_operator", "Switch workspace operator mode between user-dependent and automatic Svarog operation.", serde_json::json!({
        "type": "object",
        "properties": {
            "workspace_id": { "type": "string", "description": "Workspace id, defaults to main" },
            "operator": { "type": "string", "enum": ["user", "svarog", "auto"] }
        },
        "required": ["operator"]
    })));
    tools.push(tool_def("workspace_create_task", "Create a workspace task. New tasks are persisted in todo; assigned tasks may auto-start when the workspace operator is Svarog.", serde_json::json!({
        "type": "object",
        "properties": workspace_task_mutation_schema(),
        "required": ["title", "task_type", "description"]
    })));
    tools.push(tool_def("workspace_update_task", "Update workspace task metadata, assignee, or reviewer.", serde_json::json!({
        "type": "object",
        "properties": workspace_task_update_schema(),
        "required": ["task_id"]
    })));
    tools.push(tool_def("workspace_move_task", "Move a workspace task to another column.", serde_json::json!({
        "type": "object",
        "properties": {
            "task_id": { "type": "string" },
            "status": { "type": "string", "enum": ["todo", "in_progress", "in_review", "done"] },
            "sort_order": { "type": "integer" }
        },
        "required": ["task_id", "status"]
    })));
    for (name, description) in [
        ("workspace_run_task", "Run a workspace task using its assigned agent or subagent."),
        ("workspace_pause_task", "Pause a running workspace task where the runtime supports pause."),
        ("workspace_stop_task", "Stop a running workspace task where the runtime supports stop."),
        ("workspace_delete_task", "Soft-delete a workspace task."),
    ] {
        tools.push(tool_def(name, description, serde_json::json!({
            "type": "object",
            "properties": { "task_id": { "type": "string" } },
            "required": ["task_id"]
        })));
    }
    add_workspace_submit_review_tool(tools);
    add_workspace_submit_completion_tool(tools);
}

fn add_workspace_submit_review_tool(tools: &mut Vec<ToolDefinition>) {
    tools.push(tool_def("workspace_submit_review", "Submit a pass/fail workspace review. Failed reviews move the task back to in-progress with the review message as notice.", serde_json::json!({
        "type": "object",
        "properties": {
            "task_id": { "type": "string" },
            "verdict": { "type": "string", "enum": ["pass", "fail"] },
            "message": { "type": "string" }
        },
        "required": ["task_id", "verdict"]
    })));
}

fn add_workspace_submit_completion_tool(tools: &mut Vec<ToolDefinition>) {
    tools.push(tool_def("workspace_submit_completion", "Submit completion for an assigned in-progress workspace task. This records the delivery summary, moves the task to in-review when a reviewer is set, and queues the reviewer task for agent/subagent reviewers.", serde_json::json!({
        "type": "object",
        "properties": {
            "task_id": { "type": "string" },
            "summary": { "type": "string", "description": "Concrete summary of what was delivered for reviewer/user inspection" }
        },
        "required": ["task_id", "summary"]
    })));
}

async fn execute_workspace_task_tool(
    tool_name: &str,
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    if WORKSPACE_MUTATION_TOOLS.contains(&tool_name) && current_agent_scope_id() != MAIN_AGENT_ID {
        if tool_name == "workspace_submit_review" {
            ensure_workspace_review_scope(args, agent).await?;
        } else {
            anyhow::bail!("workspace mutation tool `{tool_name}` is only available to Svarog");
        }
    }
    if WORKSPACE_ASSIGNEE_TOOLS.contains(&tool_name) && current_agent_scope_id() != MAIN_AGENT_ID {
        ensure_workspace_completion_scope(args, agent).await?;
    }
    if !WORKSPACE_READ_TOOLS.contains(&tool_name) && !WORKSPACE_MUTATION_TOOLS.contains(&tool_name)
        && !WORKSPACE_ASSIGNEE_TOOLS.contains(&tool_name)
    {
        anyhow::bail!("unknown workspace task tool: {tool_name}");
    }
    let value = match tool_name {
        "workspace_get_settings" => {
            serde_json::to_value(agent.get_or_create_workspace_settings(&workspace_id(args)).await?)?
        }
        "workspace_list_tasks" => serde_json::to_value(
            agent
                .list_workspace_tasks(&workspace_id(args), bool_arg(args, "include_deleted"))
                .await?,
        )?,
        "workspace_get_task" => {
            serde_json::to_value(agent.get_workspace_task(&required_str(args, "task_id")?).await?)?
        }
        "workspace_list_notices" => serde_json::to_value(
            agent
                .list_workspace_notices(
                    &workspace_id(args),
                    optional_str(args, "task_id").as_deref(),
                )
                .await?,
        )?,
        "workspace_set_operator" => serde_json::to_value(
            agent
                .set_workspace_operator(&workspace_id(args), parse_operator(&required_str(args, "operator")?)?)
                .await?,
        )?,
        "workspace_create_task" => serde_json::to_value(
            agent
                .create_workspace_task(create_request(args)?, WorkspaceActor::Agent(MAIN_AGENT_ID.to_string()))
                .await?,
        )?,
        "workspace_update_task" => serde_json::to_value(
            agent
                .update_workspace_task(&required_str(args, "task_id")?, update_request(args)?)
                .await?,
        )?,
        "workspace_move_task" => serde_json::to_value(
            agent
                .move_workspace_task(WorkspaceTaskMove {
                    task_id: required_str(args, "task_id")?,
                    status: parse_status(&required_str(args, "status")?)?,
                    sort_order: args.get("sort_order").and_then(|value| value.as_i64()),
                })
                .await?,
        )?,
        "workspace_run_task" => {
            serde_json::to_value(agent.run_workspace_task(&required_str(args, "task_id")?).await?)?
        }
        "workspace_pause_task" => serde_json::to_value(
            agent.pause_workspace_task(&required_str(args, "task_id")?).await?,
        )?,
        "workspace_stop_task" => serde_json::to_value(
            agent.stop_workspace_task(&required_str(args, "task_id")?).await?,
        )?,
        "workspace_delete_task" => serde_json::to_value(
            agent.delete_workspace_task(&required_str(args, "task_id")?).await?,
        )?,
        "workspace_submit_review" => serde_json::to_value(
            agent
                .submit_workspace_review(WorkspaceReviewSubmission {
                    task_id: required_str(args, "task_id")?,
                    verdict: parse_verdict(&required_str(args, "verdict")?)?,
                    message: optional_str(args, "message"),
                })
                .await?,
        )?,
        "workspace_submit_completion" => serde_json::to_value(
            agent
                .submit_workspace_completion(
                    WorkspaceCompletionSubmission {
                        task_id: required_str(args, "task_id")?,
                        summary: required_str(args, "summary")?,
                    },
                    current_workspace_actor(),
                )
                .await?,
        )?,
        _ => unreachable!(),
    };
    Ok(serde_json::to_string_pretty(&value)?)
}

async fn ensure_workspace_review_scope(args: &serde_json::Value, agent: &AgentEngine) -> Result<()> {
    let task_id = required_str(args, "task_id")?;
    let scope_id = current_agent_scope_id();
    let Some(task) = agent.get_workspace_task(&task_id).await? else {
        anyhow::bail!("workspace task not found");
    };
    match task.reviewer {
        Some(WorkspaceActor::Agent(agent_id)) | Some(WorkspaceActor::Subagent(agent_id))
            if agent_id == scope_id =>
        {
            Ok(())
        }
        _ => anyhow::bail!(
            "workspace_submit_review is only available to Svarog or the assigned workspace reviewer"
        ),
    }
}

fn current_workspace_actor() -> WorkspaceActor {
    let scope_id = current_agent_scope_id();
    if scope_id == MAIN_AGENT_ID {
        WorkspaceActor::Agent(scope_id)
    } else {
        WorkspaceActor::Subagent(scope_id)
    }
}

async fn ensure_workspace_completion_scope(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<()> {
    let task_id = required_str(args, "task_id")?;
    let scope_id = current_agent_scope_id();
    let Some(task) = agent.get_workspace_task(&task_id).await? else {
        anyhow::bail!("workspace task not found");
    };
    match task.assignee {
        Some(WorkspaceActor::Agent(agent_id)) | Some(WorkspaceActor::Subagent(agent_id))
            if agent_id == scope_id =>
        {
            Ok(())
        }
        _ => anyhow::bail!(
            "workspace_submit_completion is only available to Svarog or the assigned workspace assignee"
        ),
    }
}

fn workspace_id(args: &serde_json::Value) -> String {
    optional_str(args, "workspace_id").unwrap_or_else(|| "main".to_string())
}

fn required_str(args: &serde_json::Value, key: &str) -> Result<String> {
    optional_str(args, key).ok_or_else(|| anyhow::anyhow!("missing '{key}' argument"))
}

fn optional_str(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bool_arg(args: &serde_json::Value, key: &str) -> bool {
    args.get(key).and_then(|value| value.as_bool()).unwrap_or(false)
}

fn workspace_task_mutation_schema() -> serde_json::Value {
    serde_json::json!({
        "workspace_id": { "type": "string", "description": "Workspace id, defaults to main" },
        "task_type": { "type": "string", "enum": ["thread", "goal"] },
        "title": { "type": "string" },
        "description": { "type": "string" },
        "definition_of_done": { "type": "string" },
        "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"] },
        "assignee": { "type": "string", "description": "user, svarog, agent:<id>, subagent:<id>, or none" },
        "reviewer": { "type": "string", "description": "user, svarog, agent:<id>, subagent:<id>, or none" }
    })
}

fn workspace_task_update_schema() -> serde_json::Value {
    let mut schema = workspace_task_mutation_schema();
    if let Some(map) = schema.as_object_mut() {
        map.insert("task_id".to_string(), serde_json::json!({ "type": "string" }));
        map.remove("task_type");
    }
    schema
}

fn create_request(args: &serde_json::Value) -> Result<WorkspaceTaskCreate> {
    Ok(WorkspaceTaskCreate {
        workspace_id: workspace_id(args),
        title: required_str(args, "title")?,
        task_type: parse_task_type(&required_str(args, "task_type")?)?,
        description: required_str(args, "description")?,
        definition_of_done: optional_str(args, "definition_of_done"),
        priority: optional_str(args, "priority")
            .map(|value| parse_priority(&value))
            .transpose()?,
        assignee: actor_arg(args, "assignee")?,
        reviewer: actor_arg(args, "reviewer")?,
    })
}

fn update_request(args: &serde_json::Value) -> Result<amux_protocol::WorkspaceTaskUpdate> {
    Ok(amux_protocol::WorkspaceTaskUpdate {
        title: optional_str(args, "title"),
        description: optional_str(args, "description"),
        definition_of_done: optional_clearable_str(args, "definition_of_done"),
        priority: optional_str(args, "priority")
            .map(|value| parse_priority(&value))
            .transpose()?,
        assignee: optional_actor_arg(args, "assignee")?,
        reviewer: optional_actor_arg(args, "reviewer")?,
    })
}

fn optional_clearable_str(args: &serde_json::Value, key: &str) -> Option<Option<String>> {
    args.get(key).map(|value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "none")
            .map(ToOwned::to_owned)
    })
}

fn actor_arg(args: &serde_json::Value, key: &str) -> Result<Option<WorkspaceActor>> {
    match optional_str(args, key) {
        Some(value) => parse_actor(&value),
        None => Ok(None),
    }
}

fn optional_actor_arg(args: &serde_json::Value, key: &str) -> Result<Option<Option<WorkspaceActor>>> {
    if !args.get(key).is_some() {
        return Ok(None);
    }
    Ok(Some(actor_arg(args, key)?))
}

fn parse_actor(value: &str) -> Result<Option<WorkspaceActor>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if value.eq_ignore_ascii_case("user") {
        return Ok(Some(WorkspaceActor::User));
    }
    if value.eq_ignore_ascii_case("svarog") || value.eq_ignore_ascii_case("swarog") {
        return Ok(Some(WorkspaceActor::Agent(MAIN_AGENT_ID.to_string())));
    }
    if let Some(id) = value.strip_prefix("agent:") {
        return Ok(Some(WorkspaceActor::Agent(id.trim().to_string())));
    }
    if let Some(id) = value.strip_prefix("subagent:") {
        return Ok(Some(WorkspaceActor::Subagent(id.trim().to_string())));
    }
    anyhow::bail!("invalid actor '{value}'")
}

fn parse_operator(value: &str) -> Result<WorkspaceOperator> {
    match value.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(WorkspaceOperator::User),
        "svarog" | "swarog" | "auto" => Ok(WorkspaceOperator::Svarog),
        other => anyhow::bail!("invalid workspace operator '{other}'"),
    }
}

fn parse_task_type(value: &str) -> Result<WorkspaceTaskType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "thread" => Ok(WorkspaceTaskType::Thread),
        "goal" => Ok(WorkspaceTaskType::Goal),
        other => anyhow::bail!("invalid workspace task type '{other}'"),
    }
}

fn parse_priority(value: &str) -> Result<WorkspacePriority> {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => Ok(WorkspacePriority::Low),
        "normal" => Ok(WorkspacePriority::Normal),
        "high" => Ok(WorkspacePriority::High),
        "urgent" => Ok(WorkspacePriority::Urgent),
        other => anyhow::bail!("invalid workspace priority '{other}'"),
    }
}

fn parse_status(value: &str) -> Result<WorkspaceTaskStatus> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "todo" => Ok(WorkspaceTaskStatus::Todo),
        "in_progress" => Ok(WorkspaceTaskStatus::InProgress),
        "in_review" => Ok(WorkspaceTaskStatus::InReview),
        "done" => Ok(WorkspaceTaskStatus::Done),
        other => anyhow::bail!("invalid workspace status '{other}'"),
    }
}

fn parse_verdict(value: &str) -> Result<WorkspaceReviewVerdict> {
    match value.trim().to_ascii_lowercase().as_str() {
        "pass" => Ok(WorkspaceReviewVerdict::Pass),
        "fail" => Ok(WorkspaceReviewVerdict::Fail),
        other => anyhow::bail!("invalid workspace review verdict '{other}'"),
    }
}
