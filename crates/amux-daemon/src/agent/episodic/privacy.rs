//! Privacy controls: PII scrubbing, TTL enforcement, session suppression.

use super::{Episode, EpisodicConfig};
use crate::scrub::scrub_sensitive;

/// Scrub sensitive data from an episode's text fields.
///
/// Applies `scrub_sensitive` to summary, root_cause, and each entity.
/// Modifies the episode in place.
pub fn scrub_episode(episode: &mut Episode) {
    episode.summary = scrub_sensitive(&episode.summary);
    if let Some(ref root_cause) = episode.root_cause {
        episode.root_cause = Some(scrub_sensitive(root_cause));
    }
    episode.entities = episode
        .entities
        .iter()
        .map(|e| scrub_sensitive(e))
        .collect();
}

/// Check if episode recording is suppressed by config.
///
/// Returns true when episodic memory is disabled or per-session suppression is active.
pub fn is_episode_suppressed(config: &EpisodicConfig) -> bool {
    !config.enabled || config.per_session_suppression
}

/// Check if an episode has expired based on its TTL.
///
/// Returns true when the episode has an `expires_at` timestamp that is in the past.
pub fn is_episode_expired(episode: &Episode, now_ms: u64) -> bool {
    match episode.expires_at {
        Some(expires) => expires < now_ms,
        None => false,
    }
}

/// Compute the expiration timestamp from a creation time and TTL in days.
///
/// Returns `None` if `ttl_days` is 0 (meaning no expiry).
pub fn compute_expires_at(created_at: u64, ttl_days: u64) -> Option<u64> {
    if ttl_days == 0 {
        None
    } else {
        Some(created_at + ttl_days * 86400 * 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::episodic::{CausalStep, EpisodeOutcome, EpisodeType};

    fn make_episode() -> Episode {
        Episode {
            id: "ep-test".to_string(),
            goal_run_id: None,
            thread_id: None,
            session_id: None,
            episode_type: EpisodeType::Discovery,
            summary: "Found api_key=sk-secret-key-12345 in config".to_string(),
            outcome: EpisodeOutcome::Success,
            root_cause: Some("Bearer abc123token was exposed".to_string()),
            entities: vec![
                "config.json".to_string(),
                "token=super_secret_value".to_string(),
            ],
            causal_chain: vec![CausalStep {
                step: "1".to_string(),
                cause: "key leak".to_string(),
                effect: "exposure".to_string(),
            }],
            solution_class: None,
            duration_ms: None,
            tokens_used: None,
            confidence: None,
            created_at: 1700000000000,
            expires_at: None,
        }
    }

    #[test]
    fn scrub_episode_replaces_api_keys_in_summary_and_root_cause() {
        let mut episode = make_episode();
        scrub_episode(&mut episode);

        // Summary should have the api_key pattern scrubbed
        assert!(
            !episode.summary.contains("sk-secret-key-12345"),
            "API key should be scrubbed from summary: {}",
            episode.summary
        );
        assert!(
            episode.summary.contains("REDACTED"),
            "Summary should contain REDACTED marker: {}",
            episode.summary
        );

        // Root cause should have Bearer token scrubbed
        let root_cause = episode.root_cause.as_ref().unwrap();
        assert!(
            !root_cause.contains("abc123token"),
            "Bearer token should be scrubbed from root_cause: {}",
            root_cause
        );
        assert!(
            root_cause.contains("REDACTED"),
            "Root cause should contain REDACTED marker: {}",
            root_cause
        );

        // Entities should have token value scrubbed
        let entity = &episode.entities[1];
        assert!(
            !entity.contains("super_secret_value"),
            "Token value should be scrubbed from entity: {}",
            entity
        );
    }

    #[test]
    fn is_episode_suppressed_returns_true_when_per_session_suppression() {
        let config = EpisodicConfig {
            enabled: true,
            per_session_suppression: true,
            ..EpisodicConfig::default()
        };
        assert!(is_episode_suppressed(&config));
    }

    #[test]
    fn is_episode_suppressed_returns_true_when_disabled() {
        let config = EpisodicConfig {
            enabled: false,
            per_session_suppression: false,
            ..EpisodicConfig::default()
        };
        assert!(is_episode_suppressed(&config));
    }

    #[test]
    fn is_episode_suppressed_returns_false_when_enabled_and_no_suppression() {
        let config = EpisodicConfig::default();
        assert!(!is_episode_suppressed(&config));
    }

    #[test]
    fn is_episode_expired_returns_true_when_expired() {
        let episode = Episode {
            expires_at: Some(1000),
            ..make_episode()
        };
        assert!(is_episode_expired(&episode, 2000));
    }

    #[test]
    fn is_episode_expired_returns_false_when_not_expired() {
        let episode = Episode {
            expires_at: Some(3000),
            ..make_episode()
        };
        assert!(!is_episode_expired(&episode, 2000));
    }

    #[test]
    fn is_episode_expired_returns_false_when_no_expiry() {
        let episode = make_episode();
        assert!(!is_episode_expired(&episode, 2000));
    }

    #[test]
    fn compute_expires_at_with_ttl() {
        let created = 1700000000000u64;
        let ttl_days = 90u64;
        let expected = created + 90 * 86400 * 1000;
        assert_eq!(compute_expires_at(created, ttl_days), Some(expected));
    }

    #[test]
    fn compute_expires_at_zero_ttl_returns_none() {
        assert_eq!(compute_expires_at(1700000000000, 0), None);
    }
}
