use super::*;

impl AgentEngine {
    pub(super) async fn try_augment_plugin_command(&self, content: &str) -> Option<String> {
        let (command_key, args) = parse_plugin_command(content)?;
        let pm = self.plugin_manager.get()?;
        let entry = pm.resolve_command(command_key).await?;
        let args_part = if args.is_empty() {
            String::new()
        } else {
            format!(" with arguments: {}", args)
        };
        if let Some(python) = &entry.python {
            return Some(format!(
                "[Plugin command: {}]\n\
                 The user invoked plugin command `{}`. \
                 Plugin: '{}'. Description: {}. \
                 Execute the following shell bootstrap with `bash_command` or `execute_command` to fulfill this request{}:\n\n```bash\n{}\n```",
                entry.command_key,
                entry.command_key,
                entry.plugin_name,
                entry.description,
                args_part,
                python.shell,
            ));
        }

        let endpoint = entry.api_endpoint.as_deref().unwrap_or("default");
        Some(format!(
            "[Plugin command: {}]\n\
             The user invoked plugin command `{}`. \
             Plugin: '{}'. Description: {}. \
             Call the plugin API endpoint '{}' for plugin '{}'{} to fulfill this request.",
            entry.command_key,
            entry.command_key,
            entry.plugin_name,
            entry.description,
            endpoint,
            entry.plugin_name,
            args_part,
        ))
    }
}

pub(super) fn parse_plugin_command(content: &str) -> Option<(&str, &str)> {
    let trimmed = content.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let (command_part, args) = match trimmed.find(' ') {
        Some(pos) => (&trimmed[..pos], trimmed[pos..].trim_start()),
        None => (trimmed, ""),
    };

    if !command_part.contains('.') {
        return None;
    }

    Some((command_part, args))
}
