use super::*;
use std::collections::BTreeSet;

const LEARNED_SHORTCUT_MIN_REQUESTS: u64 = 3;
const AUTO_APPROVE_RATE_THRESHOLD: f64 = 0.95;
const AUTO_DENY_RATE_THRESHOLD: f64 = 0.05;
const FAST_DENIAL_AUTO_DENY_THRESHOLD: u64 = 3;
const FAST_DENIAL_MAX_APPROVAL_RATE: f64 = 0.34;

pub(crate) fn count_words(content: &str) -> usize {
    content
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .count()
}

pub(crate) fn contains_confirmation_phrase(content: &str) -> bool {
    let lowered = content.to_ascii_lowercase();
    [
        "are you sure",
        "double check",
        "double-check",
        "confirm",
        "really",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RevisionSignal {
    None,
    Revision,
    Correction,
}

impl RevisionSignal {
    pub(crate) fn is_revision(self) -> bool {
        matches!(self, Self::Revision | Self::Correction)
    }

    pub(crate) fn is_correction(self) -> bool {
        matches!(self, Self::Correction)
    }
}

pub(crate) fn detect_revision_signal(content: &str) -> RevisionSignal {
    let lowered = content.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return RevisionSignal::None;
    }

    let correction_markers = [
        "actually",
        "instead",
        "rather than",
        "undo",
        "revert",
        "change that",
        "not that",
        "no, ",
        "don't do that",
    ];
    if correction_markers
        .iter()
        .any(|needle| lowered.contains(needle))
    {
        return RevisionSignal::Correction;
    }

    let revision_markers = ["use ", "prefer ", "switch to ", "next time", "better to "];
    if revision_markers
        .iter()
        .any(|needle| lowered.contains(needle))
    {
        return RevisionSignal::Revision;
    }

    RevisionSignal::None
}

pub(crate) fn current_utc_hour(timestamp_ms: u64) -> u8 {
    ((timestamp_ms / 3_600_000) % 24) as u8
}

pub(crate) fn update_running_average(current: f64, sample_count: u64, new_value: f64) -> f64 {
    if sample_count == 0 {
        return new_value;
    }
    ((current * sample_count as f64) + new_value) / (sample_count as f64 + 1.0)
}

pub(crate) fn verbosity_preference_for_length(avg_words: f64) -> VerbosityPreference {
    if avg_words < 10.0 {
        VerbosityPreference::Terse
    } else if avg_words > 35.0 {
        VerbosityPreference::Verbose
    } else {
        VerbosityPreference::Moderate
    }
}

pub(crate) fn reading_depth_for_length(avg_words: f64) -> ReadingDepth {
    if avg_words < 10.0 {
        ReadingDepth::Skim
    } else if avg_words > 35.0 {
        ReadingDepth::Deep
    } else {
        ReadingDepth::Standard
    }
}

pub(crate) fn classify_command_category(command: &str, risk_level: &str) -> &'static str {
    let lowered = command.to_ascii_lowercase();
    if lowered.contains("rm ") || lowered.contains("rm -") || lowered.contains("del ") {
        "destructive_delete"
    } else if lowered.contains("curl ")
        || lowered.contains("wget ")
        || lowered.contains("http")
        || lowered.contains("ssh ")
    {
        "network_request"
    } else if lowered.contains("git ") {
        "git"
    } else if lowered.contains("mv ")
        || lowered.contains("cp ")
        || lowered.contains("tee ")
        || lowered.contains("sed -i")
        || lowered.contains("python")
    {
        "file_write"
    } else if !risk_level.trim().is_empty() {
        match risk_level {
            "highest" => "high_risk",
            "lowest" | "yolo" => "low_risk",
            _ => "moderate_risk",
        }
    } else {
        "other"
    }
}

pub(crate) fn refresh_risk_metrics(risk: &mut RiskFingerprint) {
    risk.approval_rate_by_category = risk
        .category_requests
        .iter()
        .map(|(category, requests)| {
            let approvals = risk.category_approvals.get(category).copied().unwrap_or(0);
            let rate = if *requests == 0 {
                0.0
            } else {
                approvals as f64 / *requests as f64
            };
            (category.clone(), rate)
        })
        .collect();

    let total_resolved = risk.approvals + risk.denials;
    let approval_rate = if total_resolved == 0 {
        0.0
    } else {
        risk.approvals as f64 / total_resolved as f64
    };
    risk.risk_tolerance = if approval_rate < 0.35 {
        RiskTolerance::Conservative
    } else if approval_rate > 0.75 {
        RiskTolerance::Aggressive
    } else {
        RiskTolerance::Moderate
    };

    let mut shortcut_candidates = risk
        .category_requests
        .iter()
        .map(|(category, requests)| {
            (
                category.clone(),
                *requests,
                risk.approval_rate_by_category
                    .get(category)
                    .copied()
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    shortcut_candidates
        .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    risk.auto_approve_categories = shortcut_candidates
        .iter()
        .filter(|(_, requests, approval_rate)| {
            *requests >= LEARNED_SHORTCUT_MIN_REQUESTS
                && *approval_rate >= AUTO_APPROVE_RATE_THRESHOLD
        })
        .map(|(category, _, _)| category.clone())
        .collect();
    let mut auto_deny = shortcut_candidates
        .iter()
        .filter(|(_, requests, approval_rate)| {
            *requests >= LEARNED_SHORTCUT_MIN_REQUESTS && *approval_rate <= AUTO_DENY_RATE_THRESHOLD
        })
        .map(|(category, _, _)| category.clone())
        .collect::<BTreeSet<_>>();

    for (category, requests, approval_rate) in &shortcut_candidates {
        let fast_denials = risk
            .fast_denials_by_category
            .get(category)
            .copied()
            .unwrap_or_default();
        if *requests >= FAST_DENIAL_AUTO_DENY_THRESHOLD
            && fast_denials >= FAST_DENIAL_AUTO_DENY_THRESHOLD
            && *approval_rate <= FAST_DENIAL_MAX_APPROVAL_RATE
        {
            auto_deny.insert(category.clone());
        }
    }

    risk.auto_deny_categories = auto_deny.into_iter().collect();
}

pub(crate) fn most_common_hour(histogram: &HashMap<u8, u64>) -> Option<u8> {
    histogram
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(hour, _)| *hour)
}

pub(crate) fn top_hours(histogram: &HashMap<u8, u64>, limit: usize) -> Vec<u8> {
    let mut entries = histogram
        .iter()
        .map(|(hour, count)| (*hour, *count))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    entries
        .into_iter()
        .take(limit)
        .map(|(hour, _)| hour)
        .collect()
}

pub(crate) fn most_common_key(histogram: &HashMap<String, u64>) -> Option<String> {
    histogram
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(key, _)| key.clone())
}

pub(crate) fn top_keys(histogram: &HashMap<String, u64>, limit: usize) -> Vec<String> {
    let mut entries = histogram
        .iter()
        .map(|(key, count)| (key.clone(), *count))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    entries
        .into_iter()
        .take(limit)
        .map(|(key, _)| key)
        .collect()
}

pub(crate) fn record_attention_event(
    model: &mut OperatorModel,
    normalized_surface: &str,
    now_ms: u64,
) {
    model.attention_topology.focus_event_count += 1;
    *model
        .attention_topology
        .surface_histogram
        .entry(normalized_surface.to_string())
        .or_insert(0) += 1;

    if let (Some(previous), Some(previous_at)) = (
        model.attention_topology.last_surface.as_ref(),
        model.attention_topology.last_surface_at,
    ) {
        let dwell_secs = now_ms.saturating_sub(previous_at) / 1000;
        if dwell_secs > 0 {
            *model
                .attention_topology
                .dwell_histogram
                .entry(previous.clone())
                .or_insert(0) += dwell_secs;
            model.attention_topology.avg_surface_dwell_secs = update_running_average(
                model.attention_topology.avg_surface_dwell_secs,
                model.attention_topology.focus_event_count.saturating_sub(2),
                dwell_secs as f64,
            );
            if dwell_secs <= 15 && previous != normalized_surface {
                model.attention_topology.rapid_switch_count += 1;
            }
        }
        if previous != normalized_surface {
            let transition = format!("{previous} -> {normalized_surface}");
            *model
                .attention_topology
                .transition_histogram
                .entry(transition)
                .or_insert(0) += 1;
        }
    }

    model.attention_topology.last_surface = Some(normalized_surface.to_string());
    model.attention_topology.last_surface_at = Some(now_ms);
    model.attention_topology.dominant_surface =
        most_common_key(&model.attention_topology.surface_histogram);
    model.attention_topology.common_surfaces =
        top_keys(&model.attention_topology.surface_histogram, 3);
    model.attention_topology.top_transitions =
        top_keys(&model.attention_topology.transition_histogram, 3);
    model.attention_topology.deep_focus_surface =
        most_common_key(&model.attention_topology.dwell_histogram);
}

pub(crate) fn normalize_attention_surface(surface: &str) -> String {
    surface
        .trim()
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-') {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() {
                Some('_')
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn verbosity_label(value: VerbosityPreference) -> &'static str {
    match value {
        VerbosityPreference::Terse => "terse",
        VerbosityPreference::Moderate => "moderate",
        VerbosityPreference::Verbose => "verbose",
    }
}

pub(crate) fn risk_tolerance_label(value: RiskTolerance) -> &'static str {
    match value {
        RiskTolerance::Conservative => "conservative",
        RiskTolerance::Moderate => "moderate",
        RiskTolerance::Aggressive => "aggressive",
    }
}
