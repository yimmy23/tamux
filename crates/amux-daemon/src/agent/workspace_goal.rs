use super::*;
use amux_protocol::{WorkspaceActor, WorkspaceTask};

use super::workspace_support::{
    actor_label, reserved_goal_run_id, task_run_prompt, workspace_priority_label,
    workspace_task_priority,
};

impl AgentEngine {
    pub(in crate::agent) async fn queue_workspace_goal_run(
        &self,
        task: &WorkspaceTask,
        assignee: &WorkspaceActor,
        now: u64,
    ) -> Result<String> {
        let goal_run_id = task
            .goal_run_id
            .clone()
            .unwrap_or_else(|| reserved_goal_run_id(&task.id));
        if self.history.get_goal_run(&goal_run_id).await?.is_some() {
            return Ok(goal_run_id);
        }

        let title = task.title.clone();
        let dedicated_thread_id = format!("goal:{goal_run_id}");
        let (created_thread_id, _) = self
            .get_or_create_thread(Some(&dedicated_thread_id), &title)
            .await;
        let launch_assignment_snapshot = self.goal_launch_assignment_snapshot().await;
        let mut goal_run = GoalRun {
            id: goal_run_id.clone(),
            title,
            goal: task_run_prompt(task),
            client_request_id: Some(task.id.clone()),
            status: GoalRunStatus::Queued,
            priority: workspace_task_priority(&task.priority),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            thread_id: Some(created_thread_id.clone()),
            root_thread_id: None,
            active_thread_id: None,
            execution_thread_ids: Vec::new(),
            session_id: None,
            current_step_index: 0,
            current_step_title: None,
            current_step_kind: None,
            launch_assignment_snapshot: launch_assignment_snapshot.clone(),
            runtime_assignment_list: launch_assignment_snapshot,
            planner_owner_profile: None,
            current_step_owner_profile: None,
            replan_count: 0,
            max_replans: 2,
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            stopped_reason: None,
            child_task_ids: Vec::new(),
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
            steps: Vec::new(),
            events: vec![make_goal_run_event(
                "workspace",
                "workspace task queued goal run",
                Some(format!("workspace_task_id={}", task.id)),
            )],
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            model_usage: Vec::new(),
            autonomy_level: AutonomyLevel::default(),
            authorship_tag: Some(AuthorshipTag::Agent),
        };
        goal_run_apply_thread_routing(&mut goal_run, Some(created_thread_id.clone()));
        crate::agent::goal_dossier::refresh_goal_run_dossier(&mut goal_run);
        self.set_thread_identity_metadata(
            &created_thread_id,
            ThreadIdentityMetadata::for_goal_thread(&created_thread_id, &goal_run.id),
        )
        .await;
        self.append_system_thread_message(
            &created_thread_id,
            format!(
                "Workspace task initialized.\n\n- Task: {}\n- Priority: {}\n- Assignee: {}",
                task.title,
                workspace_priority_label(&task.priority),
                actor_label(assignee)
            ),
        )
        .await;
        self.goal_runs.lock().await.push_back(goal_run.clone());
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&goal_run, Some("Workspace goal queued".into()));
        Ok(goal_run_id)
    }
}
