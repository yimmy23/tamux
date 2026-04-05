use super::*;

#[derive(Debug, Clone)]
pub struct SentMessageResult {
    pub thread_id: String,
    pub response: String,
    pub upstream_message: Option<CompletionUpstreamMessage>,
    pub provider_final_result: Option<CompletionProviderFinalResult>,
}

impl AgentEngine {
    pub async fn send_direct_message(
        &self,
        target: &str,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<SentMessageResult> {
        if is_concierge_target(target)
            || thread_id == Some(crate::agent::concierge::CONCIERGE_THREAD_ID)
        {
            let target_thread_id = thread_id
                .unwrap_or(crate::agent::concierge::CONCIERGE_THREAD_ID)
                .to_string();
            self.send_concierge_message_on_thread(
                &target_thread_id,
                content,
                preferred_session_hint,
                true,
                true,
            )
            .await?;
            return Ok(self
                .latest_assistant_message_result(&target_thread_id)
                .await
                .unwrap_or_else(|| SentMessageResult {
                    thread_id: target_thread_id,
                    response: String::new(),
                    upstream_message: None,
                    provider_final_result: None,
                }));
        }

        let outcome = Box::pin(self.send_message_inner(
            thread_id,
            content,
            None,
            preferred_session_hint,
            None,
            None,
            None,
            None,
            true,
        ))
        .await?;
        let thread_id = outcome.thread_id.clone();
        let mut result = self
            .latest_assistant_message_result(&thread_id)
            .await
            .unwrap_or_else(|| SentMessageResult {
                thread_id: thread_id.clone(),
                response: String::new(),
                upstream_message: None,
                provider_final_result: None,
            });
        result.thread_id = thread_id;
        if result.upstream_message.is_none() {
            result.upstream_message = outcome.upstream_message.clone();
        }
        if result.provider_final_result.is_none() {
            result.provider_final_result = outcome.provider_final_result.clone();
        }
        Ok(result)
    }

    pub(in crate::agent) async fn send_internal_agent_message(
        &self,
        sender: &str,
        recipient: &str,
        content: &str,
        preferred_session_hint: Option<&str>,
    ) -> Result<SentMessageResult> {
        let wrapped = wrap_internal_message(sender, recipient, content);
        let dm_thread_id = self
            .prepare_internal_dm_thread(sender, recipient, &wrapped)
            .await;
        let outcome = if is_concierge_target(recipient) {
            Box::pin(self.send_concierge_message_on_thread(
                &dm_thread_id,
                &wrapped,
                preferred_session_hint,
                false,
                false,
            ))
            .await?;
            None
        } else {
            Some(
                Box::pin(self.send_message_inner(
                    Some(&dm_thread_id),
                    &wrapped,
                    None,
                    preferred_session_hint,
                    None,
                    None,
                    None,
                    None,
                    false,
                ))
                .await?,
            )
        };
        self.ensure_thread_identity(
            &dm_thread_id,
            &internal_dm_thread_title(sender, recipient),
            false,
        )
        .await;
        let mut result = self
            .latest_assistant_message_result(&dm_thread_id)
            .await
            .unwrap_or_else(|| SentMessageResult {
                thread_id: dm_thread_id.clone(),
                response: String::new(),
                upstream_message: None,
                provider_final_result: None,
            });
        if result.upstream_message.is_none() {
            result.upstream_message = outcome.as_ref().and_then(|value| value.upstream_message.clone());
        }
        if result.provider_final_result.is_none() {
            result.provider_final_result = outcome
                .as_ref()
                .and_then(|value| value.provider_final_result.clone());
        }
        Ok(result)
    }

    pub(in crate::agent) async fn latest_assistant_message_text(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        self.latest_assistant_message_result(thread_id)
            .await
            .map(|message| message.response)
    }

    async fn latest_assistant_message_result(&self, thread_id: &str) -> Option<SentMessageResult> {
        let threads = self.threads.read().await;
        threads.get(thread_id).and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| {
                    message.role == MessageRole::Assistant && !message.content.trim().is_empty()
                })
                .map(|message| SentMessageResult {
                    thread_id: thread_id.to_string(),
                    response: message.content.clone(),
                    upstream_message: message.upstream_message.clone(),
                    provider_final_result: message.provider_final_result.clone(),
                })
        })
    }
}