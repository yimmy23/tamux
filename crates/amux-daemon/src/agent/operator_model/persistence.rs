use super::*;

pub(crate) async fn ensure_operator_model_file(agent_data_dir: &std::path::Path) -> Result<()> {
    let path = operator_model_path(agent_data_dir);
    if !path.exists() {
        tokio::fs::create_dir_all(agent_data_dir).await?;
        let default_json = serde_json::to_string_pretty(&OperatorModel::default())?;
        tokio::fs::write(path, default_json).await?;
    }
    Ok(())
}

pub(crate) fn persist_operator_model(
    agent_data_dir: &std::path::Path,
    model: &OperatorModel,
) -> Result<()> {
    let path = operator_model_path(agent_data_dir);
    std::fs::create_dir_all(agent_data_dir)?;
    let json = serde_json::to_string_pretty(model)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub(crate) fn operator_model_path(agent_data_dir: &std::path::Path) -> std::path::PathBuf {
    agent_data_dir.join("operator_model.json")
}
