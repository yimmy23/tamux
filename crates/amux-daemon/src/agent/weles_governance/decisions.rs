use super::*;

pub(crate) fn is_suspicious_classification(classification: &WelesToolClassification) -> bool {
    matches!(classification.class, WelesGovernanceClass::GuardAlways)
        || matches!(classification.class, WelesGovernanceClass::RejectBypass)
        || !classification.reasons.is_empty()
}

pub(crate) fn should_guard_classification(classification: &WelesToolClassification) -> bool {
    match classification.class {
        WelesGovernanceClass::AllowDirect => false,
        WelesGovernanceClass::GuardIfSuspicious => !classification.reasons.is_empty(),
        WelesGovernanceClass::GuardAlways | WelesGovernanceClass::RejectBypass => true,
    }
}

pub(crate) fn direct_allow_decision(class: WelesGovernanceClass) -> WelesExecutionDecision {
    WelesExecutionDecision {
        class,
        should_execute: true,
        review: super::types::WelesReviewMeta {
            weles_reviewed: false,
            verdict: super::types::WelesVerdict::Allow,
            reasons: vec!["allow_direct: low-risk tool call".to_string()],
            audit_id: None,
            security_override_mode: None,
        },
        block_message: None,
    }
}

pub(crate) fn review_available(config: &AgentConfig) -> bool {
    config
        .extra
        .get("weles_review_available")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

pub(crate) fn bypass_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    let yolo = matches!(security_level, SecurityLevel::Yolo);
    let mut reasons = classification.reasons.clone();
    if yolo {
        reasons.push("managed security yolo downgraded bypass rejection to flag_only".to_string());
    }
    WelesExecutionDecision {
        class: classification.class,
        should_execute: yolo,
        review: super::types::WelesReviewMeta {
            weles_reviewed: true,
            verdict: if yolo {
                super::types::WelesVerdict::FlagOnly
            } else {
                super::types::WelesVerdict::Block
            },
            reasons,
            audit_id: Some(format!("weles_{}", uuid::Uuid::new_v4())),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message: if yolo {
            None
        } else {
            Some(
                "Blocked by WELES governance: shell-based Python execution must use python_execute instead."
                    .to_string(),
            )
        },
    }
}

pub(crate) fn normalize_runtime_verdict_for_classification(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
    runtime_review: WelesRuntimeReviewPayload,
) -> WelesRuntimeReviewPayload {
    if !matches!(classification.class, WelesGovernanceClass::RejectBypass) {
        return runtime_review;
    }

    WelesRuntimeReviewPayload {
        verdict: if matches!(security_level, SecurityLevel::Yolo) {
            super::types::WelesVerdict::FlagOnly
        } else {
            super::types::WelesVerdict::Block
        },
        reasons: runtime_review.reasons,
        audit_id: runtime_review.audit_id,
    }
}

pub(crate) fn unavailable_review_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    let mut reasons = classification.reasons.clone();
    reasons.push("WELES review unavailable for guarded action".to_string());
    let yolo = matches!(security_level, SecurityLevel::Yolo)
        && is_suspicious_classification(classification);
    let block_message = if yolo {
        None
    } else if reasons.is_empty() {
        Some(
            "Blocked by WELES governance: review unavailable; guarded action failed closed."
                .to_string(),
        )
    } else {
        Some(format!(
            "Blocked by WELES governance: {}",
            reasons.join("; ")
        ))
    };
    WelesExecutionDecision {
        class: classification.class,
        should_execute: yolo,
        review: super::types::WelesReviewMeta {
            weles_reviewed: false,
            verdict: if yolo {
                super::types::WelesVerdict::FlagOnly
            } else {
                super::types::WelesVerdict::Block
            },
            reasons,
            audit_id: Some(format!("weles_{}", uuid::Uuid::new_v4())),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message,
    }
}

pub(crate) fn guarded_fallback_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    if matches!(classification.class, WelesGovernanceClass::RejectBypass) {
        bypass_decision(classification, security_level)
    } else {
        unavailable_review_decision(classification, security_level)
    }
}

pub(crate) fn internal_runtime_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> WelesExecutionDecision {
    if matches!(classification.class, WelesGovernanceClass::RejectBypass) {
        let mut decision = bypass_decision(classification, security_level);
        if !decision.review.reasons.iter().any(|reason| {
            reason == "daemon-owned WELES internal scope skips recursive governance review"
        }) {
            decision.review.reasons.push(
                "daemon-owned WELES internal scope skips recursive governance review".to_string(),
            );
        }
        return decision;
    }

    let mut reasons = classification.reasons.clone();
    reasons.push("daemon-owned WELES internal scope skips recursive governance review".to_string());
    let yolo = matches!(security_level, SecurityLevel::Yolo)
        && is_suspicious_classification(classification);
    let verdict = if yolo {
        super::types::WelesVerdict::FlagOnly
    } else {
        super::types::WelesVerdict::Block
    };
    let block_message = if yolo {
        None
    } else {
        Some(format!(
            "Blocked by WELES governance: {}",
            reasons.join("; ")
        ))
    };

    WelesExecutionDecision {
        class: classification.class,
        should_execute: yolo,
        review: super::types::WelesReviewMeta {
            weles_reviewed: true,
            verdict,
            reasons,
            audit_id: Some(format!("weles_{}", uuid::Uuid::new_v4())),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message,
    }
}

pub(crate) fn build_weles_runtime_review_message(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
) -> String {
    let suspicion_summary = if classification.reasons.is_empty() {
        "none".to_string()
    } else {
        classification.reasons.join("; ")
    };
    format!(
        "Review the daemon-supplied WELES inspection context and respond with JSON only. Return a single object like {{\"verdict\":\"allow\"|\"block\",\"reasons\":[\"...\"],\"audit_id\":\"optional\"}}. Security level: {}. Governance class: {:?}. Suspicion summary: {}.",
        security_level_label(security_level),
        classification.class,
        suspicion_summary
    )
}

pub(crate) fn parse_weles_runtime_review_response(
    response: &str,
) -> Option<WelesRuntimeReviewPayload> {
    let trimmed = response.trim();
    serde_json::from_str::<WelesRuntimeReviewPayload>(trimmed)
        .ok()
        .or_else(|| {
            let start = trimmed.find('{')?;
            let end = trimmed.rfind('}')?;
            serde_json::from_str::<WelesRuntimeReviewPayload>(&trimmed[start..=end]).ok()
        })
}

pub(crate) fn reviewed_runtime_decision(
    classification: &WelesToolClassification,
    security_level: SecurityLevel,
    runtime_review: WelesRuntimeReviewPayload,
) -> WelesExecutionDecision {
    let yolo = matches!(security_level, SecurityLevel::Yolo)
        && is_suspicious_classification(classification);
    let mut reasons = if runtime_review.reasons.is_empty() {
        classification.reasons.clone()
    } else {
        runtime_review.reasons
    };
    for reason in &classification.reasons {
        if !reasons.iter().any(|existing| existing == reason) {
            reasons.push(reason.clone());
        }
    }
    if yolo {
        reasons
            .push("managed security yolo requires flag_only for suspicious tool calls".to_string());
    }

    let verdict = if yolo {
        super::types::WelesVerdict::FlagOnly
    } else if matches!(runtime_review.verdict, super::types::WelesVerdict::Block) {
        super::types::WelesVerdict::Block
    } else {
        super::types::WelesVerdict::Allow
    };
    let block_message = if matches!(verdict, super::types::WelesVerdict::Block) {
        Some(if reasons.is_empty() {
            "Blocked by WELES governance before tool execution.".to_string()
        } else {
            format!("Blocked by WELES governance: {}", reasons.join("; "))
        })
    } else {
        None
    };

    WelesExecutionDecision {
        class: classification.class,
        should_execute: !matches!(verdict, super::types::WelesVerdict::Block),
        review: super::types::WelesReviewMeta {
            weles_reviewed: true,
            verdict,
            reasons,
            audit_id: runtime_review
                .audit_id
                .or_else(|| Some(format!("weles_{}", uuid::Uuid::new_v4()))),
            security_override_mode: if yolo { Some("yolo".to_string()) } else { None },
        },
        block_message,
    }
}
