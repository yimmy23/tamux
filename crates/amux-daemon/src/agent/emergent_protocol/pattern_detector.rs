use amux_protocol::AgentDbMessage;
use std::collections::HashMap;

use super::types::{
    EmergentProtocolStore, ProtocolCandidate, ProtocolCandidateState, ProtocolObservation,
};

const DEFAULT_RECENT_WINDOW: usize = 12;
const MIN_CANDIDATE_OCCURRENCES: usize = 3;

pub(crate) fn detect_protocol_candidates(
    thread_id: &str,
    messages: &[AgentDbMessage],
) -> EmergentProtocolStore {
    let mut grouped: HashMap<String, Vec<&AgentDbMessage>> = HashMap::new();

    for message in messages.iter().rev().take(DEFAULT_RECENT_WINDOW).rev() {
        let normalized = super::compressor::compress_pattern_key(&message.content);
        if normalized.is_empty() {
            continue;
        }
        grouped.entry(normalized).or_default().push(message);
    }

    let mut candidates = Vec::new();

    for (normalized_pattern, matches) in grouped {
        if matches.len() < MIN_CANDIDATE_OCCURRENCES {
            continue;
        }
        let Some(kind) = super::decoder::classify_pattern_key(&normalized_pattern) else {
            continue;
        };

        let first_seen_at_ms = matches.first().map(|m| m.created_at as u64).unwrap_or(0);
        let last_seen_at_ms = matches.last().map(|m| m.created_at as u64).unwrap_or(0);
        let observations = matches
            .iter()
            .map(|message| ProtocolObservation {
                message_id: message.id.clone(),
                role: message.role.clone(),
                excerpt: message.content.chars().take(80).collect(),
                timestamp_ms: message.created_at as u64,
            })
            .collect::<Vec<_>>();

        let confidence = ((matches.len() as f64) / (DEFAULT_RECENT_WINDOW as f64)).min(1.0);

        candidates.push(ProtocolCandidate {
            id: format!("proto_{}", uuid::Uuid::new_v4()),
            thread_id: thread_id.to_string(),
            kind,
            trigger_phrase: matches
                .last()
                .map(|m| m.content.clone())
                .unwrap_or_else(|| normalized_pattern.clone()),
            normalized_pattern,
            state: ProtocolCandidateState::Candidate,
            confidence,
            observation_count: matches.len() as u32,
            first_seen_at_ms,
            last_seen_at_ms,
            observations,
        });
    }

    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.observation_count.cmp(&a.observation_count))
    });

    EmergentProtocolStore { candidates }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, created_at: i64, role: &str, content: &str) -> AgentDbMessage {
        AgentDbMessage {
            id: id.to_string(),
            thread_id: "thread-1".to_string(),
            created_at,
            role: role.to_string(),
            content: content.to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        }
    }

    #[test]
    fn repeated_continue_phrase_becomes_protocol_candidate() {
        let messages = vec![
            msg("m1", 1, "user", "continue"),
            msg("m2", 2, "assistant", "working"),
            msg("m3", 3, "user", "continue"),
            msg("m4", 4, "assistant", "still working"),
            msg("m5", 5, "user", "continue"),
        ];

        let store = detect_protocol_candidates("thread-1", &messages);
        assert_eq!(store.candidates.len(), 1);
        let candidate = &store.candidates[0];
        assert_eq!(candidate.normalized_pattern, "continue");
        assert_eq!(candidate.observation_count, 3);
        assert_eq!(candidate.trigger_phrase, "continue");
    }
}
