use super::*;
use crate::agent::llm_client::{
    parse_structured_upstream_failure, sanitize_upstream_failure_message,
};

mod finalize;
mod loop_core;
mod prompt;
mod setup;
mod tool_calls;
mod tool_results;
mod types;

use types::{
    FreshRunnerRetrySignal, LoopDisposition, SendMessageRunner, StreamIteration,
    ToolCallDisposition,
};

pub(super) const DEFAULT_LLM_STREAM_CHUNK_TIMEOUT_SECS: u64 = 120;

pub(super) fn tool_args_summary(arguments: &str) -> String {
    arguments.chars().take(100).collect()
}

pub(super) fn current_epoch_secs() -> u64 {
    now_millis() / 1000
}

impl AgentEngine {
    async fn run_internal_send_loop(
        &self,
        initial_thread_id: Option<&str>,
        stored_user_content: &str,
        llm_user_content: &str,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        client_surface: Option<amux_protocol::ClientSurface>,
        initial_record_operator: bool,
        initial_reuse_existing_user_message: bool,
    ) -> Result<SendMessageOutcome> {
        let mut thread_id = initial_thread_id.map(str::to_string);
        let mut record_operator = initial_record_operator;
        let mut reuse_existing_user_message = initial_reuse_existing_user_message;
        let mut scheduled_retry_cycles = 0u32;

        loop {
            let runner = SendMessageRunner::initialize(
                self,
                thread_id.as_deref(),
                stored_user_content,
                llm_user_content,
                task_id,
                preferred_session_hint,
                stream_chunk_timeout_override,
                client_surface,
                record_operator,
                reuse_existing_user_message,
                scheduled_retry_cycles,
            )
            .await?;
            let outcome = runner.run().await?;
            if let Some(retry) = outcome.fresh_runner_retry {
                thread_id = Some(outcome.thread_id);
                record_operator = false;
                reuse_existing_user_message = true;
                scheduled_retry_cycles = retry.scheduled_retry_cycles;
                continue;
            }
            return Ok(outcome);
        }
    }

    pub(in crate::agent) async fn send_message_inner(
        &self,
        thread_id: Option<&str>,
        content: &str,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        llm_user_content_override: Option<&str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        client_surface: Option<amux_protocol::ClientSurface>,
        record_operator: bool,
    ) -> Result<SendMessageOutcome> {
        let stored_user_content = content;
        let llm_user_content = llm_user_content_override.unwrap_or(content);
        let agent_scope_id = if let Some(current_task_id) = task_id {
            let tasks = self.tasks.lock().await;
            agent_scope_id_for_task(tasks.iter().find(|task| task.id == current_task_id))
        } else {
            MAIN_AGENT_ID.to_string()
        };

        Box::pin(run_with_agent_scope(agent_scope_id, async move {
            if thread_id == Some(crate::agent::concierge::CONCIERGE_THREAD_ID) {
                self.send_concierge_message_on_thread(
                    crate::agent::concierge::CONCIERGE_THREAD_ID,
                    stored_user_content,
                    preferred_session_hint,
                    record_operator,
                    true,
                )
                .await?;
                return Ok(SendMessageOutcome {
                    thread_id: crate::agent::concierge::CONCIERGE_THREAD_ID.to_string(),
                    interrupted_for_approval: false,
                    fresh_runner_retry: None,
                });
            }

            let config = self.config.read().await.clone();
            let selected_backend = backend_override
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(AgentBackend::parse)
                .unwrap_or(config.agent_backend.clone());

            match selected_backend {
                AgentBackend::Openclaw | AgentBackend::Hermes => {
                    let mut runtime_config = config.clone();
                    runtime_config.agent_backend = selected_backend;
                    return self
                        .send_message_external(&runtime_config, thread_id, llm_user_content)
                        .await
                        .map(|thread_id| SendMessageOutcome {
                            thread_id,
                            interrupted_for_approval: false,
                            fresh_runner_retry: None,
                        });
                }
                _ => {}
            }

            self.run_internal_send_loop(
                thread_id,
                stored_user_content,
                llm_user_content,
                task_id,
                preferred_session_hint,
                stream_chunk_timeout_override,
                client_surface,
                record_operator,
                false,
            )
            .await
        }))
        .await
    }

    pub(in crate::agent) async fn resend_existing_user_message(
        &self,
        thread_id: &str,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        self.run_internal_send_loop(
            Some(thread_id),
            content,
            content,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await
    }

    pub(in crate::agent) fn resolve_provider_config(
        &self,
        config: &AgentConfig,
    ) -> Result<ProviderConfig> {
        resolve_active_provider_config(config)
    }

    pub(in crate::agent) fn resolve_sub_agent_provider_config(
        &self,
        config: &AgentConfig,
        sub_agent_provider: &str,
    ) -> Result<ProviderConfig> {
        resolve_provider_config_for(config, sub_agent_provider, None)
    }
}

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn run(mut self) -> Result<SendMessageOutcome> {
        while self.max_loops == 0 || self.loop_count < self.max_loops {
            if self.stream_cancel_token.is_cancelled() {
                self.was_cancelled = true;
                break;
            }
            self.loop_count += 1;

            self.maybe_rebuild_prompt_after_memory_flush().await?;

            let iteration = match self.stream_once().await {
                Ok(iteration) => iteration,
                Err(error) => {
                    if let Some(signal) = error.downcast_ref::<types::FreshRunnerRetrySignal>() {
                        self.fresh_runner_retry = Some(FreshRunnerRetryRequest {
                            scheduled_retry_cycles: signal.scheduled_retry_cycles,
                        });
                        break;
                    }
                    if self.was_cancelled {
                        break;
                    }
                    return Err(error);
                }
            };
            if self.was_cancelled {
                break;
            }
            if self.handle_stream_timeout(&iteration).await? {
                continue;
            }
            if iteration.final_chunk.is_some() {
                self.engine
                    .record_llm_outcome(&self.config.provider, true)
                    .await;
            }
            match self.handle_iteration(iteration).await? {
                LoopDisposition::Continue => continue,
                LoopDisposition::Break => break,
            }
        }

        self.finish().await
    }
}
