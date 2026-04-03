//! Thread CRUD operations — list, get, delete, planner detection.

use super::*;

impl AgentEngine {
    pub async fn set_thread_client_surface(
        &self,
        thread_id: &str,
        client_surface: amux_protocol::ClientSurface,
    ) {
        self.thread_client_surfaces
            .write()
            .await
            .insert(thread_id.to_string(), client_surface);
        let thread_exists = self.threads.read().await.contains_key(thread_id);
        if thread_exists {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub async fn get_thread_client_surface(
        &self,
        thread_id: &str,
    ) -> Option<amux_protocol::ClientSurface> {
        self.thread_client_surfaces
            .read()
            .await
            .get(thread_id)
            .copied()
    }

    pub async fn clear_thread_client_surface(&self, thread_id: &str) {
        self.thread_client_surfaces.write().await.remove(thread_id);
    }

    pub async fn set_goal_run_client_surface(
        &self,
        goal_run_id: &str,
        client_surface: amux_protocol::ClientSurface,
    ) {
        self.goal_run_client_surfaces
            .write()
            .await
            .insert(goal_run_id.to_string(), client_surface);
    }

    pub async fn get_goal_run_client_surface(
        &self,
        goal_run_id: &str,
    ) -> Option<amux_protocol::ClientSurface> {
        self.goal_run_client_surfaces
            .read()
            .await
            .get(goal_run_id)
            .copied()
    }

    pub async fn list_threads(&self) -> Vec<AgentThread> {
        let threads = self.threads.read().await;
        let mut list: Vec<AgentThread> = threads.values().map(summarize_thread_for_list).collect();
        list.retain(|thread| {
            !crate::agent::concierge::is_user_visible_thread(thread)
                && !crate::agent::is_internal_handoff_thread(&thread.id)
        });
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    pub async fn get_thread(&self, thread_id: &str) -> Option<AgentThread> {
        self.threads
            .read()
            .await
            .get(thread_id)
            .cloned()
            .filter(|thread| {
                !crate::agent::concierge::is_user_visible_thread(thread)
                    && !crate::agent::is_internal_handoff_thread(&thread.id)
            })
    }

    pub async fn planner_required_for_thread(&self, thread_id: &str) -> bool {
        let threads = self.threads.read().await;
        let Some(thread) = threads.get(thread_id) else {
            return false;
        };
        let latest_user_message = thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.as_str())
            .unwrap_or("");
        planner_required_for_message(latest_user_message)
    }

    pub async fn delete_thread(&self, thread_id: &str) -> bool {
        let removed = self.threads.write().await.remove(thread_id).is_some();
        if removed {
            self.clear_thread_client_surface(thread_id).await;
            self.thread_handoff_states.write().await.remove(thread_id);
            self.remove_repo_watcher(thread_id).await;
            self.thread_todos.write().await.remove(thread_id);
            self.thread_work_contexts.write().await.remove(thread_id);
            self.persist_threads().await;
            self.persist_todos().await;
            self.persist_work_context().await;
        }
        removed
    }
}

fn summarize_thread_for_list(thread: &AgentThread) -> AgentThread {
    AgentThread {
        id: thread.id.clone(),
        agent_name: thread.agent_name.clone(),
        title: thread.title.clone(),
        messages: Vec::new(),
        pinned: thread.pinned,
        upstream_thread_id: thread.upstream_thread_id.clone(),
        upstream_transport: thread.upstream_transport,
        upstream_provider: thread.upstream_provider.clone(),
        upstream_model: thread.upstream_model.clone(),
        upstream_assistant_id: thread.upstream_assistant_id.clone(),
        created_at: thread.created_at,
        updated_at: thread.updated_at,
        total_input_tokens: thread.total_input_tokens,
        total_output_tokens: thread.total_output_tokens,
    }
}

#[cfg(test)]
#[path = "tests/thread_crud.rs"]
mod tests;
