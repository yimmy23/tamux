#![allow(dead_code)]

//! Divergent subagent mode — spawn 2-3 parallel framings of the same problem
//! with different system prompt perspectives, detect disagreements between their
//! outputs, surface tensions as the valuable output (not forced consensus), and
//! synthesize a mediator recommendation that acknowledges tradeoffs.

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[path = "divergent_helpers.rs"]
mod helpers;

use helpers::{format_mediator_prompt, format_tensions, generate_framing_prompts, now_millis};

// ---------------------------------------------------------------------------
// DivergentStatus
// ---------------------------------------------------------------------------

/// Status of a divergent session as it moves through its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivergentStatus {
    Spawning,
    Running,
    Mediating,
    Complete,
    Failed,
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// A single perspective/lens used to frame the problem for a subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Framing {
    pub label: String,
    pub system_prompt_override: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution_id: Option<String>,
}

// ---------------------------------------------------------------------------
// DivergentSession
// ---------------------------------------------------------------------------

/// A divergent session that manages parallel framings of a problem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergentSession {
    pub id: String,
    pub collaboration_session_id: String,
    pub problem_statement: String,
    pub framings: Vec<Framing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tensions_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mediator_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mediation_result: Option<String>,
    pub status: DivergentStatus,
    pub created_at: u64,
}

impl DivergentSession {
    /// Create a new divergent session. Validates that framings count is 2-3.
    pub fn new(problem_statement: String, framings: Vec<Framing>) -> Result<Self> {
        if framings.len() < 2 {
            anyhow::bail!(
                "divergent session requires at least 2 framings, got {}",
                framings.len()
            );
        }
        if framings.len() > 3 {
            anyhow::bail!(
                "divergent session supports at most 3 framings, got {}",
                framings.len()
            );
        }
        Ok(Self {
            id: format!("divergent_{}", uuid::Uuid::new_v4()),
            collaboration_session_id: String::new(),
            problem_statement,
            framings,
            tensions_markdown: None,
            mediator_prompt: None,
            mediation_result: None,
            status: DivergentStatus::Spawning,
            created_at: now_millis(),
        })
    }

    /// Validate and apply a status transition. Returns error on invalid transition.
    pub fn transition_to(&mut self, status: DivergentStatus) -> Result<()> {
        let valid = match (self.status, status) {
            (DivergentStatus::Spawning, DivergentStatus::Running) => true,
            (DivergentStatus::Running, DivergentStatus::Mediating) => true,
            (DivergentStatus::Mediating, DivergentStatus::Complete) => true,
            // Any state can transition to Failed
            (_, DivergentStatus::Failed) => true,
            _ => false,
        };
        if !valid {
            anyhow::bail!(
                "invalid divergent status transition: {:?} -> {:?}",
                self.status,
                status
            );
        }
        self.status = status;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AgentEngine integration
// ---------------------------------------------------------------------------

use crate::agent::engine::AgentEngine;

#[cfg(test)]
static TEST_DIVERGENT_SESSION_DELAY: std::sync::OnceLock<
    tokio::sync::Mutex<std::collections::HashMap<usize, std::time::Duration>>,
> = std::sync::OnceLock::new();

impl AgentEngine {
    #[cfg(test)]
    pub async fn set_test_divergent_session_delay(&self, delay: Option<std::time::Duration>) {
        let gate = TEST_DIVERGENT_SESSION_DELAY
            .get_or_init(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let key = self as *const AgentEngine as usize;
        let mut delays = gate.lock().await;
        if let Some(delay) = delay {
            delays.insert(key, delay);
        } else {
            delays.remove(&key);
        }
    }

    #[cfg(test)]
    pub async fn take_test_divergent_session_delay(&self) -> Option<std::time::Duration> {
        let gate = TEST_DIVERGENT_SESSION_DELAY
            .get_or_init(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let key = self as *const AgentEngine as usize;
        gate.lock().await.remove(&key)
    }

    /// Start a divergent session: create framings, set up a collaboration session,
    /// enqueue tasks for each framing, and return the session ID.
    ///
    /// The caller provides the problem statement and optionally custom framings.
    /// If no custom framings are provided, `generate_framing_prompts` is used.
    pub(crate) async fn start_divergent_session(
        &self,
        problem_statement: &str,
        custom_framings: Option<Vec<Framing>>,
        thread_id: &str,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        #[cfg(test)]
        if let Some(delay) = self.take_test_divergent_session_delay().await {
            tokio::time::sleep(delay).await;
        }

        let framings =
            custom_framings.unwrap_or_else(|| generate_framing_prompts(problem_statement));

        let mut session = DivergentSession::new(problem_statement.to_string(), framings)?;

        // Create a CollaborationSession for this divergent session.
        let collab_id = format!("collab_{}", uuid::Uuid::new_v4());
        session.collaboration_session_id = collab_id.clone();

        // Create a virtual parent task ID for the collaboration session.
        let parent_task_id = format!("divergent_parent_{}", uuid::Uuid::new_v4());

        {
            use super::super::collaboration::{CollaborationSession, CollaborativeAgent};

            let collab_session = CollaborationSession {
                id: collab_id.clone(),
                parent_task_id: parent_task_id.clone(),
                thread_id: Some(thread_id.to_string()),
                goal_run_id: goal_run_id.map(|s| s.to_string()),
                mission: problem_statement.to_string(),
                agents: session
                    .framings
                    .iter()
                    .map(|f| CollaborativeAgent {
                        task_id: f.label.clone(),
                        title: f.label.clone(),
                        role: f.label.clone(),
                        confidence: 0.5,
                        status: "spawning".to_string(),
                    })
                    .collect(),
                call_metadata: None,
                bids: Vec::new(),
                role_assignment: None,
                contributions: Vec::new(),
                disagreements: Vec::new(),
                consensus: None,
                updated_at: now_millis(),
            };
            let mut collaboration = self.collaboration.write().await;
            collaboration.insert(parent_task_id.clone(), collab_session);
        }

        // Enqueue a task for each framing.
        for framing in session.framings.iter_mut() {
            let task = self
                .enqueue_task(
                    format!("Divergent: {}", framing.label),
                    format!(
                        "{}\n\n---\nProblem: {}",
                        framing.system_prompt_override, problem_statement
                    ),
                    "normal",
                    None,
                    None,
                    Vec::new(),
                    None,
                    "divergent",
                    goal_run_id.map(|s| s.to_string()),
                    Some(parent_task_id.clone()),
                    Some(thread_id.to_string()),
                    None,
                )
                .await;
            framing.task_id = Some(task.id);
        }

        // Transition to Running.
        session.transition_to(DivergentStatus::Running)?;

        let session_id = session.id.clone();
        let mut sessions = self.divergent_sessions.write().await;
        sessions.insert(session_id.clone(), session);

        tracing::info!(
            session_id = %session_id,
            problem = %problem_statement,
            "started divergent session"
        );

        Ok(session_id)
    }

    /// Record a contribution from a framing to the divergent session.
    /// Called when each framing's task completes its work.
    pub(crate) async fn record_divergent_contribution(
        &self,
        session_id: &str,
        framing_label: &str,
        content: &str,
    ) -> Result<()> {
        self.record_divergent_contribution_internal(
            session_id,
            framing_label,
            content,
            Some(format!("contrib_{}", uuid::Uuid::new_v4())),
        )
        .await
    }

    async fn record_divergent_contribution_internal(
        &self,
        session_id: &str,
        framing_label: &str,
        content: &str,
        contribution_id: Option<String>,
    ) -> Result<()> {
        // Find the collaboration session for this divergent session.
        let parent_task_id = {
            let sessions = self.divergent_sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("unknown divergent session: {}", session_id))?;
            // Look up the collaboration session by finding the parent task ID.
            let mut found = None;
            let collab = self.collaboration.read().await;
            for (key, cs) in collab.iter() {
                if cs.id == session.collaboration_session_id {
                    found = Some(key.clone());
                    break;
                }
            }
            found.ok_or_else(|| {
                anyhow::anyhow!(
                    "no collaboration session found for divergent session {}",
                    session_id
                )
            })?
        };

        // Add contribution using framing_label as agent_id.
        {
            use super::super::collaboration::{detect_disagreements, Contribution};

            let mut collaboration = self.collaboration.write().await;
            let collab_session = collaboration.get_mut(&parent_task_id).ok_or_else(|| {
                anyhow::anyhow!("collaboration session not found for {}", parent_task_id)
            })?;

            let contribution = Contribution {
                id: contribution_id.unwrap_or_else(|| format!("contrib_{}", uuid::Uuid::new_v4())),
                task_id: framing_label.to_string(),
                topic: "primary".to_string(),
                position: content.to_string(),
                evidence: Vec::new(),
                confidence: 0.7,
                created_at: now_millis(),
            };
            collab_session.contributions.push(contribution);
            detect_disagreements(collab_session);
            collab_session.updated_at = now_millis();
        }

        Ok(())
    }

    /// Runtime hook for completed divergent tasks.
    /// Resolves session+framing from task id, records contribution output, and
    /// synthesizes session completion once all framings have contributed.
    pub(in crate::agent) async fn record_divergent_contribution_on_task_completion(
        &self,
        task: &crate::agent::types::AgentTask,
    ) -> Result<bool> {
        if task.source != "divergent" || task.status != crate::agent::types::TaskStatus::Completed {
            return Ok(false);
        }

        let resolved = {
            let sessions = self.divergent_sessions.read().await;
            sessions.iter().find_map(|(session_id, session)| {
                session
                    .framings
                    .iter()
                    .enumerate()
                    .find(|(_, framing)| framing.task_id.as_deref() == Some(task.id.as_str()))
                    .map(|(index, framing)| {
                        (
                            session_id.clone(),
                            framing.label.clone(),
                            index,
                            session
                                .framings
                                .iter()
                                .filter(|item| item.contribution_id.is_some())
                                .count(),
                            session.framings.len(),
                        )
                    })
            })
        };

        let Some((session_id, framing_label, framing_index, existing_count, total_count)) =
            resolved
        else {
            return Ok(false);
        };

        let contribution_text = task
            .result
            .as_deref()
            .or(task
                .logs
                .iter()
                .rev()
                .find(|entry| !entry.message.trim().is_empty())
                .map(|entry| entry.message.as_str()))
            .unwrap_or(task.description.as_str());
        let contribution_text = contribution_text.trim();
        if contribution_text.is_empty() {
            return Ok(false);
        }

        let contribution_id = format!("contrib_{}", uuid::Uuid::new_v4());
        let should_complete = {
            let mut sessions = self.divergent_sessions.write().await;
            let Some(session) = sessions.get_mut(&session_id) else {
                return Ok(false);
            };
            let Some(framing) = session.framings.get_mut(framing_index) else {
                return Ok(false);
            };
            if framing.contribution_id.is_some() {
                return Ok(true);
            }
            framing.contribution_id = Some(contribution_id.clone());
            existing_count + 1 == total_count
        };

        self.record_divergent_contribution_internal(
            &session_id,
            &framing_label,
            contribution_text,
            Some(contribution_id),
        )
        .await?;

        if should_complete {
            let _ = self.complete_divergent_session(&session_id).await?;
        }

        Ok(true)
    }

    /// Canonical divergent session payload for operator retrieval surfaces.
    pub(crate) async fn get_divergent_session(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value> {
        let session = {
            let sessions = self.divergent_sessions.read().await;
            sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unknown divergent session: {}", session_id))?
        };
        let total = session.framings.len();
        let completed = session
            .framings
            .iter()
            .filter(|framing| framing.contribution_id.is_some())
            .count();

        Ok(serde_json::json!({
            "session_id": session.id,
            "status": session.status,
            "problem_statement": session.problem_statement,
            "framing_progress": {
                "completed": completed,
                "total": total,
                "all_contributed": completed == total
            },
            "framings": session.framings.iter().map(|framing| serde_json::json!({
                "label": framing.label,
                "task_id": framing.task_id,
                "has_contribution": framing.contribution_id.is_some(),
                "contribution_id": framing.contribution_id,
            })).collect::<Vec<_>>(),
            "tensions_markdown": session.tensions_markdown,
            "mediator_prompt": session.mediator_prompt,
            "mediation_result": session.mediation_result,
            "created_at": session.created_at,
        }))
    }

    /// Complete a divergent session: detect disagreements, format tensions,
    /// generate mediator prompt, and return the prompt.
    ///
    /// NOTE: The actual LLM mediator call is triggered by the caller (goal runner
    /// or agent loop). This method prepares the prompt and returns it. The caller
    /// decides whether to make the LLM call or present tensions directly to the
    /// operator.
    pub(crate) async fn complete_divergent_session(&self, session_id: &str) -> Result<String> {
        // Look up divergent session.
        let (collab_session_id, framings, existing_prompt) = {
            let sessions = self.divergent_sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("unknown divergent session: {}", session_id))?;
            if session.status == DivergentStatus::Complete {
                if let Some(prompt) = session.mediator_prompt.clone() {
                    return Ok(prompt);
                }
            }
            (
                session.collaboration_session_id.clone(),
                session.framings.clone(),
                session.mediator_prompt.clone(),
            )
        };

        // Find the parent_task_id for the collaboration session.
        let parent_task_id = {
            let collab = self.collaboration.read().await;
            let mut found = None;
            for (key, cs) in collab.iter() {
                if cs.id == collab_session_id {
                    found = Some(key.clone());
                    break;
                }
            }
            found.ok_or_else(|| {
                anyhow::anyhow!(
                    "no collaboration session found for divergent session {}",
                    session_id
                )
            })?
        };

        // Detect disagreements and format tensions.
        let tensions = {
            use super::super::collaboration::detect_disagreements;

            let mut collaboration = self.collaboration.write().await;
            let collab_session = collaboration.get_mut(&parent_task_id).ok_or_else(|| {
                anyhow::anyhow!("collaboration session not found for {}", parent_task_id)
            })?;

            detect_disagreements(collab_session);
            format_tensions(&collab_session.disagreements, &framings)
        };

        // Generate mediator prompt and update session.
        let mediator_prompt = {
            let mut sessions = self.divergent_sessions.write().await;
            let session = sessions
                .get_mut(session_id)
                .ok_or_else(|| anyhow::anyhow!("unknown divergent session: {}", session_id))?;

            if session.status == DivergentStatus::Running {
                session.transition_to(DivergentStatus::Mediating)?;
            }
            let prompt =
                existing_prompt.unwrap_or_else(|| format_mediator_prompt(session, &tensions));
            session.tensions_markdown = Some(tensions.clone());
            session.mediator_prompt = Some(prompt.clone());
            if session.status == DivergentStatus::Mediating {
                session.transition_to(DivergentStatus::Complete)?;
            }
            prompt
        };

        tracing::info!(
            session_id = %session_id,
            "completed divergent session — mediator prompt generated"
        );

        Ok(mediator_prompt)
    }
}

#[cfg(test)]
#[path = "tests/divergent.rs"]
mod tests;
