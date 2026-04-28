use super::helpers::{
    command_family, pattern_family_from_factor, summarize_outcome, FamilyOutcomeSummary,
};
use super::*;

impl AgentEngine {
    pub(in crate::agent) async fn build_causal_guidance_summary(&self) -> Option<String> {
        let mut advisories = Vec::new();
        advisories.extend(
            self.build_pattern_guidance_for_option(
                "execute_managed_command",
                48,
                "execute_managed_command",
            )
            .await?,
        );
        advisories.extend(
            self.build_pattern_guidance_for_option("bash_command", 48, "bash_command")
                .await?,
        );
        advisories.extend(
            self.build_pattern_guidance_for_option("upstream_recovery", 24, "upstream recovery")
                .await?,
        );

        if let Ok(records) = self
            .history
            .list_recent_causal_trace_records("replan_after_failure", 24)
            .await
        {
            let mut recovery_patterns = Vec::new();
            for record in records {
                let Ok(outcome) = serde_json::from_str::<
                    crate::agent::learning::traces::CausalTraceOutcome,
                >(&record.outcome_json) else {
                    continue;
                };
                let Some(summary) = summarize_outcome(outcome) else {
                    continue;
                };
                if !summary.is_near_miss {
                    continue;
                }
                if let Some(recovery) = summary.recovery {
                    if recovery_patterns.len() < 2 {
                        recovery_patterns.push(crate::agent::summarize_text(&recovery, 110));
                    }
                }
            }
            if !recovery_patterns.is_empty() {
                advisories.push(format!(
                    "- recovery: recent near-miss replans recovered via {}",
                    recovery_patterns.join("; ")
                ));
            }
        }

        if advisories.is_empty() {
            None
        } else {
            advisories.sort();
            advisories.truncate(3);
            Some(format!(
                "## Recent Causal Guidance\n{}",
                advisories.join("\n")
            ))
        }
    }

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

impl AgentEngine {
    async fn build_pattern_guidance_for_option(
        &self,
        option_type: &str,
        limit: u32,
        label: &str,
    ) -> Option<Vec<String>> {
        let records = self
            .history
            .list_recent_causal_trace_records(option_type, limit)
            .await
            .ok()?;
        let mut by_family: HashMap<String, FamilyOutcomeSummary> = HashMap::new();
        for record in records {
            let Ok(factors) = serde_json::from_str::<
                Vec<crate::agent::learning::traces::CausalFactor>,
            >(&record.causal_factors_json) else {
                continue;
            };
            let Some(family) = factors.iter().find_map(pattern_family_from_factor) else {
                continue;
            };
            let Ok(outcome) = serde_json::from_str::<
                crate::agent::learning::traces::CausalTraceOutcome,
            >(&record.outcome_json) else {
                continue;
            };
            let Some(summary) = summarize_outcome(outcome) else {
                continue;
            };
            by_family.entry(family).or_default().record(summary);
        }

        let mut advisories = Vec::new();
        for (family, summary) in by_family {
            let caution_count = summary.failure_count + summary.near_miss_count;
            if caution_count == 0 {
                continue;
            }
            let recent_reasons = summary
                .reasons
                .into_iter()
                .map(|reason| crate::agent::summarize_text(&reason, 100))
                .collect::<Vec<_>>()
                .join("; ");
            let recovery_hint = if summary.recoveries.is_empty() {
                String::new()
            } else {
                format!(
                    "; recent recovery pattern: {}",
                    summary
                        .recoveries
                        .into_iter()
                        .map(|recovery| crate::agent::summarize_text(&recovery, 100))
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            };
            advisories.push(format!(
                "- {} / {}: {} failure(s), {} near-miss(es); watch for {}{}",
                label,
                family,
                summary.failure_count,
                summary.near_miss_count,
                recent_reasons,
                recovery_hint
            ));
        }
        advisories.sort();
        Some(advisories)
    }
}
