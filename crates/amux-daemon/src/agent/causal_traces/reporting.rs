use super::helpers::{
    command_family, pattern_family_from_factor, summarize_outcome, FamilyOutcomeSummary,
};
use super::*;

impl AgentEngine {
    pub async fn causal_trace_report(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let rows = self
            .history
            .list_causal_traces_for_option(option_type, limit.max(1))
            .await?;

        let mut success = 0u32;
        let mut failure = 0u32;
        let mut near_miss = 0u32;
        let mut unresolved = 0u32;
        let mut recent_failure_reasons = Vec::new();
        let mut recent_recoveries = Vec::new();

        for row in rows {
            let Ok(outcome) =
                serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(&row)
            else {
                continue;
            };
            match outcome {
                crate::agent::learning::traces::CausalTraceOutcome::Success => success += 1,
                crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
                    failure += 1;
                    if recent_failure_reasons.len() < 5 {
                        recent_failure_reasons.push(reason);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
                    what_went_wrong,
                    how_recovered,
                } => {
                    near_miss += 1;
                    if recent_failure_reasons.len() < 5 {
                        recent_failure_reasons.push(what_went_wrong);
                    }
                    if recent_recoveries.len() < 5 {
                        recent_recoveries.push(how_recovered);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::Unresolved => unresolved += 1,
            }
        }

        let resolved = success + failure + near_miss;
        let success_rate = if resolved == 0 {
            0.0
        } else {
            success as f64 / resolved as f64
        };

        Ok(serde_json::json!({
            "option_type": option_type,
            "sample_size": success + failure + near_miss + unresolved,
            "success": success,
            "failure": failure,
            "near_miss": near_miss,
            "unresolved": unresolved,
            "success_rate": success_rate,
            "recent_failure_reasons": recent_failure_reasons,
            "recent_recoveries": recent_recoveries,
        }))
    }

    pub async fn counterfactual_report(
        &self,
        option_type: &str,
        family_hint: &str,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let normalized_family = command_family(family_hint);
        let records = self
            .history
            .list_recent_causal_trace_records(option_type, limit.max(1))
            .await?;
        let mut success = 0u32;
        let mut failure = 0u32;
        let mut near_miss = 0u32;
        let mut unresolved = 0u32;
        let mut recent_reasons = Vec::new();
        let mut recent_recoveries = Vec::new();

        for record in records {
            let Ok(factors) = serde_json::from_str::<
                Vec<crate::agent::learning::traces::CausalFactor>,
            >(&record.causal_factors_json) else {
                continue;
            };
            let Some(family) = factors.iter().find_map(pattern_family_from_factor) else {
                continue;
            };
            if family != normalized_family {
                continue;
            }

            let Ok(outcome) = serde_json::from_str::<
                crate::agent::learning::traces::CausalTraceOutcome,
            >(&record.outcome_json) else {
                continue;
            };
            match outcome {
                crate::agent::learning::traces::CausalTraceOutcome::Success => success += 1,
                crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
                    failure += 1;
                    if recent_reasons.len() < 4 {
                        recent_reasons.push(reason);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
                    what_went_wrong,
                    how_recovered,
                } => {
                    near_miss += 1;
                    if recent_reasons.len() < 4 {
                        recent_reasons.push(what_went_wrong);
                    }
                    if recent_recoveries.len() < 4 {
                        recent_recoveries.push(how_recovered);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::Unresolved => unresolved += 1,
            }
        }

        let resolved = success + failure + near_miss;
        let likely_risk = if failure + near_miss >= 3 {
            "high"
        } else if failure + near_miss >= 1 {
            "medium"
        } else {
            "low"
        };
        let recommendation = if resolved == 0 {
            "no matching historical evidence"
        } else if success > failure + near_miss {
            "history leans favorable, but review recent cautions"
        } else {
            "history leans risky; prefer tighter scoping or approval-first execution"
        };

        Ok(serde_json::json!({
            "option_type": option_type,
            "command_family": normalized_family,
            "sample_size": success + failure + near_miss + unresolved,
            "success": success,
            "failure": failure,
            "near_miss": near_miss,
            "unresolved": unresolved,
            "likely_risk": likely_risk,
            "recommendation": recommendation,
            "recent_failure_reasons": recent_reasons,
            "recent_recoveries": recent_recoveries,
        }))
    }
}
