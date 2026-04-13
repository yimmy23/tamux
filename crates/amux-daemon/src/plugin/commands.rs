#![allow(dead_code)]

//! Plugin command registry: namespaced slash commands declared in plugin manifests.

use std::collections::HashMap;

use super::loader::LoadedPlugin;

/// A registered plugin command entry.
#[derive(Debug, Clone)]
pub(crate) struct PluginCommandEntry {
    pub plugin_name: String,
    pub command_key: String,
    pub description: String,
    pub api_endpoint: Option<String>,
}

/// Registry of all plugin slash commands. Rebuilt when plugins change.
pub(crate) struct PluginCommandRegistry {
    commands: HashMap<String, PluginCommandEntry>,
}

impl PluginCommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Clear and repopulate from all loaded plugins.
    /// For each plugin with commands, creates entries with key `/pluginname.commandname` per PSKL-05.
    pub fn rebuild_from_plugins(&mut self, plugins: &HashMap<String, LoadedPlugin>) {
        self.commands.clear();
        for (plugin_name, loaded) in plugins {
            let Some(cmds) = &loaded.manifest.commands else {
                continue;
            };
            for (cmd_name, cmd_def) in cmds {
                let key = format!("/{}.{}", plugin_name, cmd_name);
                self.commands.insert(
                    key.clone(),
                    PluginCommandEntry {
                        plugin_name: plugin_name.clone(),
                        command_key: key,
                        description: cmd_def.description.clone(),
                        api_endpoint: cmd_def.action.clone(),
                    },
                );
            }
        }
    }

    /// Resolve a user input string to a command entry.
    /// Checks if input starts with a registered command key (exact match or followed by whitespace).
    pub fn resolve(&self, input: &str) -> Option<&PluginCommandEntry> {
        for (key, entry) in &self.commands {
            if input == key || input.starts_with(&format!("{} ", key)) {
                return Some(entry);
            }
        }
        None
    }

    /// Return all entries sorted by command_key.
    pub fn list_all(&self) -> Vec<&PluginCommandEntry> {
        let mut entries: Vec<&PluginCommandEntry> = self.commands.values().collect();
        entries.sort_by(|a, b| a.command_key.cmp(&b.command_key));
        entries
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manifest::{CommandDef, PluginManifest};

    fn make_plugin_with_commands(
        name: &str,
        commands: Vec<(&str, &str, Option<&str>)>,
    ) -> LoadedPlugin {
        let mut cmd_map = HashMap::new();
        for (cmd_name, desc, action) in commands {
            cmd_map.insert(
                cmd_name.to_string(),
                CommandDef {
                    description: desc.to_string(),
                    action: action.map(|a| a.to_string()),
                },
            );
        }
        LoadedPlugin {
            manifest: PluginManifest {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                schema_version: 1,
                description: None,
                author: None,
                license: None,
                tamux_version: None,
                settings: None,
                api: None,
                commands: Some(cmd_map),
                skills: None,
                auth: None,
                extra: HashMap::new(),
            },
            manifest_json: String::new(),
            dir_name: name.to_string(),
        }
    }

    #[test]
    fn rebuild_populates_namespaced_commands() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();
        plugins.insert(
            "gmail-calendar".to_string(),
            make_plugin_with_commands(
                "gmail-calendar",
                vec![
                    ("inbox", "Show inbox", Some("list_messages")),
                    ("send", "Send email", Some("send_message")),
                ],
            ),
        );

        registry.rebuild_from_plugins(&plugins);

        assert!(!registry.is_empty());
        let all = registry.list_all();
        assert_eq!(all.len(), 2);
        // Commands should be namespaced as /pluginname.commandname
        assert!(all.iter().any(|e| e.command_key == "/gmail-calendar.inbox"));
        assert!(all.iter().any(|e| e.command_key == "/gmail-calendar.send"));
    }

    #[test]
    fn resolve_finds_registered_command() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();
        plugins.insert(
            "gmail-calendar".to_string(),
            make_plugin_with_commands(
                "gmail-calendar",
                vec![("inbox", "Show inbox", Some("list_messages"))],
            ),
        );
        registry.rebuild_from_plugins(&plugins);

        let entry = registry.resolve("/gmail-calendar.inbox").unwrap();
        assert_eq!(entry.plugin_name, "gmail-calendar");
        assert_eq!(entry.api_endpoint.as_deref(), Some("list_messages"));
    }

    #[test]
    fn resolve_returns_none_for_unregistered() {
        let mut registry = PluginCommandRegistry::new();
        let plugins: HashMap<String, LoadedPlugin> = HashMap::new();
        registry.rebuild_from_plugins(&plugins);

        assert!(registry.resolve("/unknown.command").is_none());
    }

    #[test]
    fn list_all_returns_sorted_entries() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();
        plugins.insert(
            "weather".to_string(),
            make_plugin_with_commands("weather", vec![("forecast", "Get forecast", None)]),
        );
        plugins.insert(
            "gmail".to_string(),
            make_plugin_with_commands("gmail", vec![("inbox", "Show inbox", Some("list"))]),
        );
        registry.rebuild_from_plugins(&plugins);

        let all = registry.list_all();
        assert_eq!(all.len(), 2);
        // Sorted: /gmail.inbox < /weather.forecast
        assert_eq!(all[0].command_key, "/gmail.inbox");
        assert_eq!(all[1].command_key, "/weather.forecast");
    }
}
