//! Thread CRUD operations — list, get, delete, planner detection.

use super::*;

impl AgentEngine {
    pub async fn list_threads(&self) -> Vec<AgentThread> {
        let threads = self.threads.read().await;
        let mut list: Vec<AgentThread> = threads.values().cloned().collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    pub async fn get_thread(&self, thread_id: &str) -> Option<AgentThread> {
        self.threads.read().await.get(thread_id).cloned()
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
