use anyhow::Result;

use crate::agent::engine::AgentEngine;
use crate::history::{CognitiveBiasRow, WorkflowProfileRow};

use super::types::{CognitiveBias, SelfModel, TriggerPattern, WorkflowProfile};

impl AgentEngine {
    pub(crate) async fn load_meta_cognitive_self_model(&self) -> Result<SelfModel> {
        let Some(model_row) = self.history.get_meta_cognition_model().await? else {
            let mut model = SelfModel::default();
            model.last_updated_ms = crate::agent::task_prompt::now_millis();
            self.persist_meta_cognitive_self_model(&model).await?;
            return Ok(model);
        };

        let biases = self
            .history
            .list_cognitive_biases(model_row.id)
            .await?
            .into_iter()
            .map(|row| {
                Ok(CognitiveBias {
                    name: row.name,
                    trigger_pattern: serde_json::from_str::<TriggerPattern>(
                        &row.trigger_pattern_json,
                    )?,
                    severity: row.severity,
                    mitigation_prompt: row.mitigation_prompt,
                    occurrence_count: row.occurrence_count,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let workflow_profiles = self
            .history
            .list_workflow_profiles(model_row.id)
            .await?
            .into_iter()
            .map(|row| {
                Ok(WorkflowProfile {
                    name: row.name,
                    avg_success_rate: row.avg_success_rate,
                    avg_steps: row.avg_steps as u32,
                    typical_tools: serde_json::from_str::<Vec<String>>(&row.typical_tools_json)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(SelfModel {
            agent_id: model_row.agent_id,
            calibration_offset: model_row.calibration_offset,
            biases,
            workflow_profiles,
            last_updated_ms: model_row.last_updated_at,
        })
    }

    pub(crate) async fn persist_meta_cognitive_self_model(&self, model: &SelfModel) -> Result<()> {
        let updated_at = model
            .last_updated_ms
            .max(crate::agent::task_prompt::now_millis());
        self.history
            .upsert_meta_cognition_model(&model.agent_id, model.calibration_offset, updated_at)
            .await?;

        let model_row = self
            .history
            .get_meta_cognition_model()
            .await?
            .ok_or_else(|| anyhow::anyhow!("meta-cognition model row missing after upsert"))?;

        let bias_rows = model
            .biases
            .iter()
            .map(|bias| {
                Ok(CognitiveBiasRow {
                    id: 0,
                    model_id: model_row.id,
                    name: bias.name.clone(),
                    trigger_pattern_json: serde_json::to_string(&bias.trigger_pattern)?,
                    mitigation_prompt: bias.mitigation_prompt.clone(),
                    severity: bias.severity,
                    occurrence_count: bias.occurrence_count,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.history
            .replace_cognitive_biases(model_row.id, &bias_rows)
            .await?;

        let workflow_rows = model
            .workflow_profiles
            .iter()
            .map(|profile| {
                Ok(WorkflowProfileRow {
                    id: 0,
                    model_id: model_row.id,
                    name: profile.name.clone(),
                    avg_success_rate: profile.avg_success_rate,
                    avg_steps: profile.avg_steps as u64,
                    typical_tools_json: serde_json::to_string(&profile.typical_tools)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.history
            .replace_workflow_profiles(model_row.id, &workflow_rows)
            .await?;

        Ok(())
    }
}
