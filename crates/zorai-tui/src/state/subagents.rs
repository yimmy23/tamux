#![allow(dead_code)]

#[path = "subagents_parts/visible_for_provider_to_normalize_role_preset_id.rs"]
mod visible_for_provider_to_normalize_role_preset_id;

#[path = "subagents_parts/new_to_reduce.rs"]
mod new_to_reduce;

pub use new_to_reduce::*;
pub use visible_for_provider_to_normalize_role_preset_id::*;

#[cfg(test)]
mod tests {
    use super::{find_role_preset, role_picker_index_for_id, SUBAGENT_ROLE_PRESETS};

    #[test]
    fn role_presets_include_execution_and_task_scope_roles() {
        let ids: Vec<&str> = SUBAGENT_ROLE_PRESETS
            .iter()
            .map(|preset| preset.id)
            .collect();

        assert!(ids.contains(&"executor"));
        assert!(ids.contains(&"implementation"));
        assert!(ids.contains(&"debugging"));
        assert!(ids.contains(&"architecture"));
        assert!(ids.contains(&"security"));
        assert!(ids.contains(&"data_analysis"));
        assert!(ids.contains(&"medical_research"));
        assert!(ids.contains(&"financial_analysis"));
        assert!(ids.contains(&"legal_research"));
        assert!(ids.contains(&"art_direction"));
        assert!(ids.contains(&"scientific_review"));
        assert!(ids.contains(&"product_marketing"));
        assert!(ids.contains(&"writing"));
        assert!(ids.contains(&"coordination"));
        assert!(ids.contains(&"product_strategy"));
        assert!(ids.contains(&"operations"));
        assert!(!ids.contains(&"technical"));
        assert!(!ids.contains(&"non_technical"));
        assert_eq!(
            find_role_preset("executor").map(|preset| preset.label),
            Some("Executor / Performer")
        );
        assert!(role_picker_index_for_id("performer").is_some());
        assert!(role_picker_index_for_id("product_strategy").is_some());
    }
}
