//! Shared agent identity helpers for main, concierge, and spawned agents.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use amux_protocol::{AGENT_ID_RAROG, AGENT_ID_SWAROG, AGENT_NAME_RAROG, AGENT_NAME_SWAROG};

use super::types::AgentTask;

pub(super) const MAIN_AGENT_ID: &str = AGENT_ID_SWAROG;
pub(super) const MAIN_AGENT_NAME: &str = AGENT_NAME_SWAROG;
pub(super) const CONCIERGE_AGENT_ID: &str = AGENT_ID_RAROG;
pub(super) const CONCIERGE_AGENT_NAME: &str = AGENT_NAME_RAROG;
pub(super) const INTERNAL_DM_THREAD_PREFIX: &str = "dm:";
pub(super) const PERSONA_MARKER: &str = "Agent persona:";
pub(super) const MAIN_AGENT_ALIAS: &str = "main";
pub(super) const MAIN_AGENT_LEGACY_ALIAS: &str = "main-agent";
pub(super) const MAIN_AGENT_FALLBACK_ALIAS: &str = "assistant";
pub(super) const CONCIERGE_AGENT_ALIAS: &str = "concierge";
pub(super) const CONCIERGE_AGENT_LEGACY_ALIAS: &str = "concierge-agent";

pub(super) const SWAROZYC_AGENT_ID: &str = "swarozyc";
pub(super) const SWAROZYC_AGENT_NAME: &str = "Swarozyc";
pub(super) const RADOGOST_AGENT_ID: &str = "radogost";
pub(super) const RADOGOST_AGENT_NAME: &str = "Radogost";
pub(super) const DOMOWOJ_AGENT_ID: &str = "domowoj";
pub(super) const DOMOWOJ_AGENT_NAME: &str = "Domowoj";
pub(super) const SWIETOWIT_AGENT_ID: &str = "swietowit";
pub(super) const SWIETOWIT_AGENT_NAME: &str = "Swietowit";
pub(super) const ROD_AGENT_ID: &str = "rod";
pub(super) const ROD_AGENT_NAME: &str = "Rod";
pub(super) const WELES_AGENT_ID: &str = "weles";
pub(super) const WELES_AGENT_NAME: &str = "Weles";

struct PersonaSeed {
    id: &'static str,
    name: &'static str,
    guidance: &'static str,
}

const SPAWNED_PERSONAS: [PersonaSeed; 6] = [
    PersonaSeed {
        id: SWAROZYC_AGENT_ID,
        name: SWAROZYC_AGENT_NAME,
        guidance: "You inherit the main agent's craft but stay narrower, quicker, and more execution-focused than the main agent.",
    },
    PersonaSeed {
        id: RADOGOST_AGENT_ID,
        name: RADOGOST_AGENT_NAME,
        guidance: "You specialize in negotiation between options, comparing tradeoffs, and surfacing the strongest route forward.",
    },
    PersonaSeed {
        id: DOMOWOJ_AGENT_ID,
        name: DOMOWOJ_AGENT_NAME,
        guidance: "You are a careful keeper of the working environment. Favor stability, cleanup, and precise local fixes.",
    },
    PersonaSeed {
        id: SWIETOWIT_AGENT_ID,
        name: SWIETOWIT_AGENT_NAME,
        guidance: "You maintain broader situational awareness than most subagents and should keep the surrounding architecture in view.",
    },
    PersonaSeed {
        id: ROD_AGENT_ID,
        name: ROD_AGENT_NAME,
        guidance: "You are continuity-minded. Prefer solutions that preserve durable structure, conventions, and long-term coherence.",
    },
    PersonaSeed {
        id: WELES_AGENT_ID,
        name: WELES_AGENT_NAME,
        guidance: "You are comfortable exploring edge cases, failure modes, and messy corners, but you must report back clearly and concretely.",
    },
];

pub(super) fn canonical_agent_id(alias: &str) -> &'static str {
    let normalized = alias.trim().to_ascii_lowercase();
    match normalized.as_str() {
        MAIN_AGENT_ID | MAIN_AGENT_ALIAS | MAIN_AGENT_LEGACY_ALIAS | MAIN_AGENT_FALLBACK_ALIAS => {
            MAIN_AGENT_ID
        }
        CONCIERGE_AGENT_ID | CONCIERGE_AGENT_ALIAS | CONCIERGE_AGENT_LEGACY_ALIAS => {
            CONCIERGE_AGENT_ID
        }
        _ => MAIN_AGENT_ID,
    }
}

pub(super) fn canonical_agent_name(alias: &str) -> &'static str {
    match canonical_agent_id(alias) {
        CONCIERGE_AGENT_ID => CONCIERGE_AGENT_NAME,
        _ => MAIN_AGENT_NAME,
    }
}

pub(super) fn is_concierge_target(alias: &str) -> bool {
    canonical_agent_id(alias) == CONCIERGE_AGENT_ID
}

pub(super) fn internal_dm_thread_id(agent_a: &str, agent_b: &str) -> String {
    let mut ids = [
        canonical_agent_id(agent_a).to_string(),
        canonical_agent_id(agent_b).to_string(),
    ];
    ids.sort();
    format!("{INTERNAL_DM_THREAD_PREFIX}{}:{}", ids[0], ids[1])
}

pub(super) fn internal_dm_thread_title(agent_a: &str, agent_b: &str) -> String {
    let mut names = [
        canonical_agent_name(agent_a).to_string(),
        canonical_agent_name(agent_b).to_string(),
    ];
    names.sort();
    format!("Internal DM · {} ↔ {}", names[0], names[1])
}

pub(super) fn is_internal_dm_thread(thread_id: &str) -> bool {
    thread_id.starts_with(INTERNAL_DM_THREAD_PREFIX)
}

pub(super) fn spawned_persona_name(seed: &str) -> &'static str {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let idx = (hasher.finish() as usize) % SPAWNED_PERSONAS.len();
    SPAWNED_PERSONAS[idx].name
}

pub(super) fn build_spawned_persona_prompt(seed: &str) -> String {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let idx = (hasher.finish() as usize) % SPAWNED_PERSONAS.len();
    let persona = &SPAWNED_PERSONAS[idx];
    format!(
        "{PERSONA_MARKER} {}\nYou are {} ({}) operating as a spawned tamux agent.\n{}\n{} is the main agent. {} is {}'s concierge. Know who they are, do not impersonate them, and coordinate with them when that helps move the task forward.\nKeep your personality distinct but concise, pragmatic, and production-focused.",
        persona.name,
        persona.name,
        persona.id,
        persona.guidance,
        MAIN_AGENT_NAME,
        CONCIERGE_AGENT_NAME,
        MAIN_AGENT_NAME,
    )
}

pub(super) fn extract_persona_name(system_prompt: Option<&str>) -> Option<String> {
    system_prompt.and_then(|prompt| {
        prompt.lines().find_map(|line| {
            line.trim()
                .strip_prefix(PERSONA_MARKER)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
    })
}

pub(super) fn sender_name_for_task(task: Option<&AgentTask>) -> String {
    task.and_then(|item| extract_persona_name(item.override_system_prompt.as_deref()))
        .unwrap_or_else(|| MAIN_AGENT_NAME.to_string())
}

pub(super) fn wrap_internal_message(sender: &str, recipient: &str, content: &str) -> String {
    format!(
        "Internal agent message from {} to {}.\nRespond directly to the request below and assume the recipient will relay or integrate your answer.\n\n{}",
        canonical_agent_name(sender),
        canonical_agent_name(recipient),
        content.trim()
    )
}

pub(super) fn concierge_should_escalate(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let keywords = [
        "code",
        "coding",
        "rust",
        "typescript",
        "react",
        "debug",
        "bug",
        "build",
        "compile",
        "test",
        "refactor",
        "implement",
        "file",
        "patch",
        "database",
        "schema",
        "migration",
        "daemon",
        "cli",
        "review",
    ];
    lower.len() > 220 || keywords.iter().any(|keyword| lower.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_dm_thread_id_is_stable_and_sorted() {
        assert_eq!(
            internal_dm_thread_id(CONCIERGE_AGENT_ID, MAIN_AGENT_ID),
            internal_dm_thread_id(MAIN_AGENT_ID, CONCIERGE_AGENT_ID)
        );
        assert_eq!(
            internal_dm_thread_id(MAIN_AGENT_ALIAS, CONCIERGE_AGENT_ALIAS),
            "dm:rarog:swarog"
        );
    }

    #[test]
    fn spawned_persona_prompt_is_stable_for_seed() {
        let first = build_spawned_persona_prompt("task-123");
        let second = build_spawned_persona_prompt("task-123");
        assert_eq!(first, second);
        assert!(first.contains(PERSONA_MARKER));
    }

    #[test]
    fn canonical_ids_follow_aliases() {
        assert_eq!(canonical_agent_id(MAIN_AGENT_ALIAS), MAIN_AGENT_ID);
        assert_eq!(
            canonical_agent_id(CONCIERGE_AGENT_ALIAS),
            CONCIERGE_AGENT_ID
        );
        assert_eq!(canonical_agent_id(MAIN_AGENT_FALLBACK_ALIAS), MAIN_AGENT_ID);
    }
}
