use super::*;

fn protected_weles_integrity_reason(definition: &SubAgentDefinition) -> Option<String> {
    if !definition.enabled {
        return Some("daemon-owned WELES definition is disabled".to_string());
    }
    if !definition.builtin
        || !definition.immutable_identity
        || definition.disable_allowed
        || definition.delete_allowed
    {
        return Some("daemon-owned WELES protection metadata drifted".to_string());
    }
    None
}

fn should_emit_weles_health_update(
    previous: &WelesHealthStatus,
    next: &WelesHealthStatus,
) -> bool {
    previous.state != next.state || previous.reason != next.reason
}

impl AgentEngine {
    async fn compute_weles_health_status(&self, checked_at: u64) -> WelesHealthStatus {
        let config = self.config.read().await.clone();
        if !crate::agent::weles_governance::review_available(&config) {
            return WelesHealthStatus {
                state: WelesHealthState::Degraded,
                reason: Some("WELES review unavailable for guarded actions".to_string()),
                checked_at,
            };
        }

        let subagents = self.list_sub_agents().await;
        let Some(weles) = subagents
            .iter()
            .find(|entry| entry.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
        else {
            return WelesHealthStatus {
                state: WelesHealthState::Degraded,
                reason: Some("daemon-owned WELES definition missing".to_string()),
                checked_at,
            };
        };

        if let Some(reason) = protected_weles_integrity_reason(weles) {
            return WelesHealthStatus {
                state: WelesHealthState::Degraded,
                reason: Some(reason),
                checked_at,
            };
        }

        WelesHealthStatus {
            state: WelesHealthState::Healthy,
            reason: None,
            checked_at,
        }
    }

    pub(super) async fn refresh_weles_health_from_heartbeat(
        &self,
        checked_at: u64,
    ) -> WelesHealthStatus {
        let next = self.compute_weles_health_status(checked_at).await;
        let mut current = self.weles_health.write().await;
        let emit = should_emit_weles_health_update(&current, &next);
        *current = next.clone();
        drop(current);

        if emit {
            let _ = self.event_tx.send(AgentEvent::WelesHealthUpdate {
                state: next.state,
                reason: next.reason.clone(),
                checked_at: next.checked_at,
            });
        }

        next
    }
}
