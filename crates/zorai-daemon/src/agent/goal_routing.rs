use super::*;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedGoalLocalAgent {
    pub role_id: String,
    pub agent_label: String,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum GoalResolvedAgentTarget {
    BuiltinMain,
    GoalLocal(ResolvedGoalLocalAgent),
    GlobalSubagent(SubAgentDefinition),
}

impl GoalResolvedAgentTarget {
    #[cfg(test)]
    pub(crate) fn role_id(&self) -> &str {
        match self {
            Self::BuiltinMain => crate::agent::agent_identity::MAIN_AGENT_ID,
            Self::GoalLocal(agent) => agent.role_id.as_str(),
            Self::GlobalSubagent(definition) => definition.id.as_str(),
        }
    }

    #[cfg(test)]
    pub(crate) fn is_goal_local(&self) -> bool {
        matches!(self, Self::GoalLocal(_))
    }

    #[cfg(test)]
    pub(crate) fn is_global_subagent(&self) -> bool {
        matches!(self, Self::GlobalSubagent(_))
    }
}

fn is_main_identifier(identifier: &str) -> bool {
    let normalized = identifier.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        crate::agent::agent_identity::MAIN_AGENT_ID
            | crate::agent::agent_identity::MAIN_AGENT_PUBLIC_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_LEGACY_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_FALLBACK_ALIAS
    )
}

fn normalize_role_token(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn heuristic_role_aliases(normalized: &str) -> &'static [&'static str] {
    match normalized {
        "plan" | "planner" | "planning" | "strategist" | "strategy" => {
            &["planning", "planner", "plan", "strategy", "strategist"]
        }
        "research" | "researcher" | "investigator" => &["research", "researcher", "investigator"],
        "verify" | "verifier" | "review" | "reviewer" | "qa" => {
            &["verifier", "verify", "review", "reviewer", "qa"]
        }
        _ => &[],
    }
}

fn matches_global_subagent(definition: &SubAgentDefinition, identifier: &str) -> bool {
    definition.id.eq_ignore_ascii_case(identifier)
        || definition.name.eq_ignore_ascii_case(identifier)
        || definition
            .role
            .as_deref()
            .is_some_and(|role| role.eq_ignore_ascii_case(identifier))
        || definition
            .id
            .strip_suffix("_builtin")
            .is_some_and(|value| value.eq_ignore_ascii_case(identifier))
}

fn goal_local_agent_label(role_id: &str) -> String {
    role_id
        .split(['_', '-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn exact_goal_local_match(
    identifier: &str,
    assignments: &[GoalAgentAssignment],
) -> Option<GoalResolvedAgentTarget> {
    assignments
        .iter()
        .find(|assignment| {
            assignment.enabled && assignment.role_id.eq_ignore_ascii_case(identifier)
        })
        .map(|assignment| {
            GoalResolvedAgentTarget::GoalLocal(ResolvedGoalLocalAgent {
                role_id: assignment.role_id.clone(),
                agent_label: goal_local_agent_label(&assignment.role_id),
                provider: assignment.provider.clone(),
                model: assignment.model.clone(),
                reasoning_effort: assignment.reasoning_effort.clone(),
            })
        })
}

fn heuristic_goal_local_match(
    identifier: &str,
    assignments: &[GoalAgentAssignment],
) -> Option<GoalResolvedAgentTarget> {
    let normalized = normalize_role_token(identifier);
    let aliases = heuristic_role_aliases(&normalized);
    if aliases.is_empty() {
        return None;
    }
    assignments
        .iter()
        .find(|assignment| {
            assignment.enabled
                && aliases.iter().any(|alias| {
                    let candidate = normalize_role_token(&assignment.role_id);
                    candidate == *alias
                })
        })
        .map(|assignment| {
            GoalResolvedAgentTarget::GoalLocal(ResolvedGoalLocalAgent {
                role_id: assignment.role_id.clone(),
                agent_label: goal_local_agent_label(&assignment.role_id),
                provider: assignment.provider.clone(),
                model: assignment.model.clone(),
                reasoning_effort: assignment.reasoning_effort.clone(),
            })
        })
}

fn exact_or_heuristic_goal_local_match(
    identifier: &str,
    assignments: &[GoalAgentAssignment],
) -> Option<ResolvedGoalLocalAgent> {
    match exact_goal_local_match(identifier, assignments)
        .or_else(|| heuristic_goal_local_match(identifier, assignments))
    {
        Some(GoalResolvedAgentTarget::GoalLocal(agent)) => Some(agent),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) fn resolve_goal_binding_candidate(
    identifier: &str,
    _step_title: &str,
    _step_instructions: &str,
    assignments: &[GoalAgentAssignment],
    global_subagents: &[SubAgentDefinition],
) -> Option<GoalResolvedAgentTarget> {
    if is_main_identifier(identifier) {
        return Some(GoalResolvedAgentTarget::BuiltinMain);
    }
    exact_goal_local_match(identifier, assignments)
        .or_else(|| heuristic_goal_local_match(identifier, assignments))
        .or_else(|| {
            global_subagents
                .iter()
                .find(|definition| {
                    definition.enabled && matches_global_subagent(definition, identifier)
                })
                .cloned()
                .map(GoalResolvedAgentTarget::GlobalSubagent)
        })
}

pub(crate) fn goal_local_agent_prompt_block(assignments: &[GoalAgentAssignment]) -> String {
    assignments
        .iter()
        .filter(|assignment| assignment.enabled)
        .filter(|assignment| !is_main_identifier(&assignment.role_id))
        .map(|assignment| {
            let mut line = format!(
                "- {}: provider={}, model={}",
                assignment.role_id, assignment.provider, assignment.model
            );
            if let Some(reasoning_effort) = assignment
                .reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                line.push_str(", reasoning=");
                line.push_str(reasoning_effort);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn goal_run_step_execution_binding(
    goal_run: &GoalRun,
    step: &GoalRunStep,
) -> Option<GoalRoleBinding> {
    goal_run
        .dossier
        .as_ref()?
        .units
        .iter()
        .find(|unit| unit.id == step.id)
        .map(|unit| unit.execution_binding.clone())
}

#[derive(Debug, Deserialize)]
struct GoalLocalRoleMatch {
    #[serde(default)]
    selected_role_id: Option<String>,
}

impl AgentEngine {
    async fn request_goal_local_role_match(
        &self,
        identifier: &str,
        step_title: &str,
        step_instructions: &str,
        assignments: &[GoalAgentAssignment],
    ) -> Option<String> {
        let available_roles = assignments
            .iter()
            .filter(|assignment| assignment.enabled)
            .map(|assignment| {
                format!(
                    "- role_id={} provider={} model={}",
                    assignment.role_id, assignment.provider, assignment.model
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if available_roles.is_empty() {
            return None;
        }

        let prompt = format!(
            "Choose the best goal-local role for this requested binding.\n\
             Return strict JSON only in the form {{\"selected_role_id\":string|null}}.\n\
             Pick only from the available role ids below. If none fit, return null.\n\n\
             Requested binding: {}\n\
             Step title: {}\n\
             Step instructions: {}\n\n\
             Available role ids:\n{}",
            identifier, step_title, step_instructions, available_roles
        );

        let selection = self
            .run_goal_structured::<GoalLocalRoleMatch>(&prompt)
            .await
            .ok()?;
        let selected_role_id = selection
            .selected_role_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())?;

        assignments
            .iter()
            .find(|assignment| {
                assignment.enabled && assignment.role_id.eq_ignore_ascii_case(selected_role_id)
            })
            .map(|assignment| assignment.role_id.clone())
    }

    pub(crate) async fn resolve_goal_local_binding_with_fallback(
        &self,
        identifier: &str,
        step_title: &str,
        step_instructions: &str,
        assignments: &[GoalAgentAssignment],
    ) -> Option<ResolvedGoalLocalAgent> {
        if let Some(local) = exact_or_heuristic_goal_local_match(identifier, assignments) {
            return Some(local);
        }
        let selected_role_id = self
            .request_goal_local_role_match(identifier, step_title, step_instructions, assignments)
            .await?;
        exact_or_heuristic_goal_local_match(&selected_role_id, assignments)
    }

    pub(crate) async fn resolve_goal_target_for_binding(
        &self,
        goal_run: &GoalRun,
        step: &GoalRunStep,
        binding: &GoalRoleBinding,
    ) -> Option<GoalResolvedAgentTarget> {
        let identifier = match binding {
            GoalRoleBinding::Builtin(value) | GoalRoleBinding::Subagent(value) => value,
        };
        if is_main_identifier(identifier) {
            return Some(GoalResolvedAgentTarget::BuiltinMain);
        }
        if let Some(local) = self
            .resolve_goal_local_binding_with_fallback(
                identifier,
                &step.title,
                &step.instructions,
                &goal_run.runtime_assignment_list,
            )
            .await
        {
            return Some(GoalResolvedAgentTarget::GoalLocal(local));
        }
        let global_subagents = self.list_sub_agents().await;
        global_subagents
            .iter()
            .find(|definition| {
                definition.enabled && matches_global_subagent(definition, identifier)
            })
            .cloned()
            .map(GoalResolvedAgentTarget::GlobalSubagent)
    }

    pub(crate) async fn resolve_goal_execution_target(
        &self,
        goal_run: &GoalRun,
        step: &GoalRunStep,
    ) -> Option<GoalResolvedAgentTarget> {
        let binding =
            goal_run_step_execution_binding(goal_run, step).or_else(|| match &step.kind {
                GoalRunStepKind::Specialist(role) if !role.trim().is_empty() => {
                    Some(GoalRoleBinding::Subagent(role.clone()))
                }
                _ => None,
            })?;
        self.resolve_goal_target_for_binding(goal_run, step, &binding)
            .await
    }

    pub(crate) async fn apply_goal_resolved_target_to_task(
        &self,
        task_id: &str,
        target: Option<&GoalResolvedAgentTarget>,
    ) -> Option<AgentTask> {
        let mut mark_trusted_weles = None;
        let updated = {
            let mut tasks = self.tasks.lock().await;
            let task = tasks.iter_mut().find(|task| task.id == task_id)?;
            match target {
                Some(GoalResolvedAgentTarget::GoalLocal(agent)) => {
                    task.override_provider = Some(agent.provider.clone());
                    task.override_model = Some(agent.model.clone());
                    task.sub_agent_def_id = None;
                }
                Some(GoalResolvedAgentTarget::GlobalSubagent(definition)) => {
                    task.override_provider = Some(definition.provider.clone());
                    task.override_model = Some(definition.model.clone());
                    task.override_system_prompt = definition.system_prompt.clone();
                    task.tool_whitelist = definition.tool_whitelist.clone();
                    task.tool_blacklist = definition.tool_blacklist.clone();
                    task.context_budget_tokens = definition.context_budget_tokens;
                    task.max_duration_secs = definition.max_duration_secs;
                    task.supervisor_config = definition.supervisor_config.clone();
                    task.sub_agent_def_id = Some(definition.id.clone());
                    crate::agent::task_crud::enforce_goal_task_autonomy_tool_blacklist(task);
                    if definition.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID {
                        mark_trusted_weles = Some(task.id.clone());
                    }
                }
                Some(GoalResolvedAgentTarget::BuiltinMain) | None => {}
            }
            task.clone()
        };
        if let Some(task_id) = mark_trusted_weles {
            self.trusted_weles_tasks.write().await.insert(task_id);
        }
        self.persist_tasks().await;
        Some(updated)
    }

    pub(crate) async fn goal_owner_profile_for_task_target(
        &self,
        task: &AgentTask,
        target: Option<&GoalResolvedAgentTarget>,
    ) -> GoalRuntimeOwnerProfile {
        match target {
            Some(GoalResolvedAgentTarget::GoalLocal(agent)) => GoalRuntimeOwnerProfile {
                agent_label: agent.agent_label.clone(),
                provider: agent.provider.clone(),
                model: agent.model.clone(),
                reasoning_effort: agent.reasoning_effort.clone(),
            },
            _ => self.current_step_owner_profile_for_task(task).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_assignment(role_id: &str, provider: &str, model: &str) -> GoalAgentAssignment {
        GoalAgentAssignment {
            role_id: role_id.to_string(),
            enabled: true,
            provider: provider.to_string(),
            model: model.to_string(),
            reasoning_effort: Some("medium".to_string()),
            inherit_from_main: false,
        }
    }

    fn sample_subagent(id: &str, name: &str) -> SubAgentDefinition {
        SubAgentDefinition {
            id: id.to_string(),
            name: name.to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: None,
            system_prompt: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            max_duration_secs: None,
            supervisor_config: None,
            enabled: true,
            builtin: false,
            immutable_identity: false,
            disable_allowed: true,
            delete_allowed: true,
            protected_reason: None,
            reasoning_effort: Some("medium".to_string()),
            created_at: 0,
        }
    }

    #[test]
    fn goal_local_resolver_prefers_exact_role_id_match() {
        let assignments = vec![sample_assignment("planning", "openai", "gpt-5.4-mini")];
        let resolved = resolve_goal_binding_candidate(
            "planning",
            "Plan the rollout",
            "Break work into steps",
            &assignments,
            &[],
        )
        .expect("resolver should match");
        assert_eq!(resolved.role_id(), "planning");
        assert!(resolved.is_goal_local());
    }

    #[test]
    fn goal_local_resolver_uses_heuristic_alias_match() {
        let assignments = vec![sample_assignment("research", "openai", "gpt-5.4-mini")];
        let resolved = resolve_goal_binding_candidate(
            "researcher",
            "Collect facts",
            "Search docs and summarize",
            &assignments,
            &[],
        )
        .expect("resolver should match");
        assert_eq!(resolved.role_id(), "research");
    }

    #[test]
    fn goal_local_resolver_falls_back_to_global_subagent_when_local_missing() {
        let globals = vec![sample_subagent("android-verifier", "Android Verifier")];
        let resolved = resolve_goal_binding_candidate(
            "android-verifier",
            "Verify build",
            "Run checks",
            &[],
            &globals,
        )
        .expect("resolver should match");
        assert!(resolved.is_global_subagent());
    }

    #[test]
    fn goal_local_agent_prompt_block_omits_main_and_lists_enabled_local_roles() {
        let block = goal_local_agent_prompt_block(&[
            sample_assignment(
                crate::agent::agent_identity::MAIN_AGENT_ID,
                "openai",
                "gpt-5.4",
            ),
            sample_assignment("planning", "openai", "gpt-5.4-mini"),
            GoalAgentAssignment {
                enabled: false,
                ..sample_assignment("research", "openai", "gpt-5.4")
            },
        ]);

        assert!(block.contains("planning"));
        assert!(!block.contains(crate::agent::agent_identity::MAIN_AGENT_ID));
        assert!(!block.contains("research"));
    }
}
