//! Tool filtering for sub-agents — restrict which tools a sub-agent may call.
//!
//! Supports whitelist-only, blacklist-only, or combined modes with conflict
//! detection when both are provided.

use crate::agent::types::ToolDefinition;

/// Filter that restricts which tools a sub-agent may invoke.
#[derive(Debug, Clone)]
pub struct ToolFilter {
    whitelist: Option<Vec<String>>,
    blacklist: Option<Vec<String>>,
}

/// Conflict detected when a tool appears in both whitelist and blacklist.
#[derive(Debug, Clone)]
pub struct ToolFilterConflict {
    pub conflicting_tools: Vec<String>,
}

impl std::fmt::Display for ToolFilterConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tool filter conflict: tools appear in both whitelist and blacklist: {}",
            self.conflicting_tools.join(", ")
        )
    }
}

impl ToolFilter {
    /// Create a new tool filter from optional whitelist and blacklist.
    ///
    /// Returns an error if any tool names appear in both lists.
    pub fn new(
        whitelist: Option<Vec<String>>,
        blacklist: Option<Vec<String>>,
    ) -> Result<Self, ToolFilterConflict> {
        if let (Some(wl), Some(bl)) = (&whitelist, &blacklist) {
            let conflicts: Vec<String> = wl
                .iter()
                .filter(|tool| bl.contains(tool))
                .cloned()
                .collect();
            if !conflicts.is_empty() {
                return Err(ToolFilterConflict {
                    conflicting_tools: conflicts,
                });
            }
        }

        Ok(Self {
            whitelist: whitelist.filter(|v| !v.is_empty()),
            blacklist: blacklist.filter(|v| !v.is_empty()),
        })
    }

    /// Create a permissive filter that allows all tools.
    pub fn allow_all() -> Self {
        Self {
            whitelist: None,
            blacklist: None,
        }
    }

    /// Create a restrictive filter that denies every tool.
    pub fn deny_all() -> Self {
        Self {
            whitelist: Some(Vec::new()),
            blacklist: None,
        }
    }

    /// Check whether a specific tool is allowed by this filter.
    pub fn is_allowed(&self, tool_name: &str) -> bool {
        if let Some(whitelist) = &self.whitelist {
            if !whitelist.iter().any(|t| t == tool_name) {
                return false;
            }
        }
        if let Some(blacklist) = &self.blacklist {
            if blacklist.iter().any(|t| t == tool_name) {
                return false;
            }
        }
        true
    }

    /// Filter a list of tool definitions, returning only allowed tools.
    pub fn filtered_tools(&self, tools: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
        tools
            .into_iter()
            .filter(|tool| self.is_allowed(&tool.function.name))
            .collect()
    }

    /// Ensure specific tools remain available even when a task or sub-agent
    /// profile uses a whitelist/blacklist for its normal tool surface.
    pub fn allow_tools<I, S>(&mut self, tools: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let required = tools
            .into_iter()
            .map(|tool| tool.as_ref().to_string())
            .collect::<Vec<_>>();
        if required.is_empty() {
            return;
        }

        if let Some(blacklist) = &mut self.blacklist {
            blacklist.retain(|tool| !required.iter().any(|required| required == tool));
        }

        if let Some(whitelist) = &mut self.whitelist {
            for tool in required {
                if !whitelist.iter().any(|existing| existing == &tool) {
                    whitelist.push(tool);
                }
            }
        }
    }

    /// If a tool is denied, return a human-readable reason.
    pub fn deny_reason(&self, tool_name: &str) -> Option<String> {
        if let Some(whitelist) = &self.whitelist {
            if !whitelist.iter().any(|t| t == tool_name) {
                return Some(format!(
                    "tool '{}' is not in the whitelist: [{}]",
                    tool_name,
                    whitelist.join(", ")
                ));
            }
        }
        if let Some(blacklist) = &self.blacklist {
            if blacklist.iter().any(|t| t == tool_name) {
                return Some(format!("tool '{}' is blacklisted", tool_name));
            }
        }
        None
    }

    /// Returns true if this filter imposes any restrictions.
    pub fn has_restrictions(&self) -> bool {
        self.whitelist.is_some() || self.blacklist.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ToolDefinition, ToolFunctionDef};

    fn make_tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: name.to_string(),
                description: format!("Tool {name}"),
                parameters: serde_json::json!({}),
            },
        }
    }

    // --- Construction ---

    #[test]
    fn allow_all_permits_everything() {
        let filter = ToolFilter::allow_all();
        assert!(filter.is_allowed(zorai_protocol::tool_names::BASH_COMMAND));
        assert!(filter.is_allowed(zorai_protocol::tool_names::READ_FILE));
        assert!(!filter.has_restrictions());
    }

    #[test]
    fn deny_all_blocks_everything() {
        let filter = ToolFilter::deny_all();
        assert!(!filter.is_allowed(zorai_protocol::tool_names::BASH_COMMAND));
        assert!(!filter.is_allowed(zorai_protocol::tool_names::READ_FILE));
        assert!(filter.has_restrictions());
        assert_eq!(
            filter
                .deny_reason(zorai_protocol::tool_names::BASH_COMMAND)
                .as_deref(),
            Some("tool 'bash_command' is not in the whitelist: []")
        );
    }

    #[test]
    fn empty_whitelist_treated_as_no_restriction() {
        let filter = ToolFilter::new(Some(vec![]), None).unwrap();
        assert!(filter.is_allowed("any_tool"));
        assert!(!filter.has_restrictions());
    }

    #[test]
    fn empty_blacklist_treated_as_no_restriction() {
        let filter = ToolFilter::new(None, Some(vec![])).unwrap();
        assert!(filter.is_allowed("any_tool"));
        assert!(!filter.has_restrictions());
    }

    #[test]
    fn conflict_detection_rejects_overlapping_lists() {
        let result = ToolFilter::new(
            Some(vec![
                zorai_protocol::tool_names::BASH_COMMAND.into(),
                zorai_protocol::tool_names::READ_FILE.into(),
            ]),
            Some(vec![
                zorai_protocol::tool_names::READ_FILE.into(),
                zorai_protocol::tool_names::WRITE_FILE.into(),
            ]),
        );
        let err = result.unwrap_err();
        assert_eq!(
            err.conflicting_tools,
            vec![zorai_protocol::tool_names::READ_FILE]
        );
    }

    #[test]
    fn no_conflict_when_lists_are_disjoint() {
        let filter = ToolFilter::new(
            Some(vec![zorai_protocol::tool_names::BASH_COMMAND.into()]),
            Some(vec![zorai_protocol::tool_names::WRITE_FILE.into()]),
        );
        assert!(filter.is_ok());
    }

    // --- Whitelist-only ---

    #[test]
    fn whitelist_allows_listed_tools() {
        let filter = ToolFilter::new(
            Some(vec![
                zorai_protocol::tool_names::BASH_COMMAND.into(),
                zorai_protocol::tool_names::READ_FILE.into(),
            ]),
            None,
        )
        .unwrap();
        assert!(filter.is_allowed(zorai_protocol::tool_names::BASH_COMMAND));
        assert!(filter.is_allowed(zorai_protocol::tool_names::READ_FILE));
        assert!(filter.has_restrictions());
    }

    #[test]
    fn whitelist_blocks_unlisted_tools() {
        let filter = ToolFilter::new(
            Some(vec![zorai_protocol::tool_names::BASH_COMMAND.into()]),
            None,
        )
        .unwrap();
        assert!(!filter.is_allowed(zorai_protocol::tool_names::WRITE_FILE));
        assert!(!filter.is_allowed(zorai_protocol::tool_names::SEARCH_FILES));
    }

    // --- Blacklist-only ---

    #[test]
    fn blacklist_blocks_listed_tools() {
        let filter = ToolFilter::new(
            None,
            Some(vec![
                zorai_protocol::tool_names::BASH_COMMAND.into(),
                zorai_protocol::tool_names::WRITE_FILE.into(),
            ]),
        )
        .unwrap();
        assert!(!filter.is_allowed(zorai_protocol::tool_names::BASH_COMMAND));
        assert!(!filter.is_allowed(zorai_protocol::tool_names::WRITE_FILE));
    }

    #[test]
    fn blacklist_allows_unlisted_tools() {
        let filter = ToolFilter::new(
            None,
            Some(vec![zorai_protocol::tool_names::BASH_COMMAND.into()]),
        )
        .unwrap();
        assert!(filter.is_allowed(zorai_protocol::tool_names::READ_FILE));
        assert!(filter.is_allowed(zorai_protocol::tool_names::SEARCH_FILES));
    }

    // --- Combined ---

    #[test]
    fn combined_whitelist_and_blacklist() {
        let filter = ToolFilter::new(
            Some(vec![
                zorai_protocol::tool_names::BASH_COMMAND.into(),
                zorai_protocol::tool_names::READ_FILE.into(),
                zorai_protocol::tool_names::LIST_FILES.into(),
            ]),
            Some(vec![zorai_protocol::tool_names::BASH_COMMAND.into()]),
        );
        // bash_command is in both → conflict
        assert!(filter.is_err());
    }

    #[test]
    fn combined_disjoint_filters_correctly() {
        let filter = ToolFilter::new(
            Some(vec![
                zorai_protocol::tool_names::READ_FILE.into(),
                zorai_protocol::tool_names::LIST_FILES.into(),
            ]),
            Some(vec![zorai_protocol::tool_names::WRITE_FILE.into()]),
        )
        .unwrap();
        assert!(filter.is_allowed(zorai_protocol::tool_names::READ_FILE));
        assert!(filter.is_allowed(zorai_protocol::tool_names::LIST_FILES));
        // Not in whitelist:
        assert!(!filter.is_allowed(zorai_protocol::tool_names::BASH_COMMAND));
        // In blacklist (but also not in whitelist, so blocked by whitelist first):
        assert!(!filter.is_allowed(zorai_protocol::tool_names::WRITE_FILE));
    }

    // --- filtered_tools ---

    #[test]
    fn filtered_tools_returns_only_allowed() {
        let filter = ToolFilter::new(
            Some(vec![
                zorai_protocol::tool_names::READ_FILE.into(),
                zorai_protocol::tool_names::LIST_FILES.into(),
            ]),
            None,
        )
        .unwrap();
        let all_tools = vec![
            make_tool(zorai_protocol::tool_names::READ_FILE),
            make_tool(zorai_protocol::tool_names::WRITE_FILE),
            make_tool(zorai_protocol::tool_names::LIST_FILES),
            make_tool(zorai_protocol::tool_names::BASH_COMMAND),
        ];
        let filtered = filter.filtered_tools(all_tools);
        let names: Vec<&str> = filtered.iter().map(|t| t.function.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                zorai_protocol::tool_names::READ_FILE,
                zorai_protocol::tool_names::LIST_FILES
            ]
        );
    }

    #[test]
    fn filtered_tools_returns_all_when_no_restrictions() {
        let filter = ToolFilter::allow_all();
        let tools = vec![make_tool("a"), make_tool("b"), make_tool("c")];
        let filtered = filter.filtered_tools(tools);
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn allow_tools_extends_whitelist_and_removes_blacklist_entries() {
        let mut filter = ToolFilter::new(
            Some(vec![zorai_protocol::tool_names::CANCEL_TASK.into()]),
            Some(vec!["deny_me".into()]),
        )
        .unwrap();

        filter.allow_tools([
            zorai_protocol::tool_names::WORKSPACE_SUBMIT_REVIEW,
            zorai_protocol::tool_names::WORKSPACE_LIST_TASKS,
            "deny_me",
        ]);

        assert!(filter.is_allowed(zorai_protocol::tool_names::CANCEL_TASK));
        assert!(filter.is_allowed(zorai_protocol::tool_names::WORKSPACE_SUBMIT_REVIEW));
        assert!(filter.is_allowed(zorai_protocol::tool_names::WORKSPACE_LIST_TASKS));
        assert!(filter.is_allowed("deny_me"));
        assert!(!filter.is_allowed(zorai_protocol::tool_names::WORKSPACE_CREATE_TASK));
    }

    // --- deny_reason ---

    #[test]
    fn deny_reason_none_for_allowed_tool() {
        let filter = ToolFilter::new(
            Some(vec![zorai_protocol::tool_names::BASH_COMMAND.into()]),
            None,
        )
        .unwrap();
        assert!(filter
            .deny_reason(zorai_protocol::tool_names::BASH_COMMAND)
            .is_none());
    }

    #[test]
    fn deny_reason_whitelist_explains_missing() {
        let filter = ToolFilter::new(
            Some(vec![zorai_protocol::tool_names::BASH_COMMAND.into()]),
            None,
        )
        .unwrap();
        let reason = filter
            .deny_reason(zorai_protocol::tool_names::WRITE_FILE)
            .unwrap();
        assert!(reason.contains("not in the whitelist"));
        assert!(reason.contains(zorai_protocol::tool_names::BASH_COMMAND));
    }

    #[test]
    fn deny_reason_blacklist_explains_blocked() {
        let filter = ToolFilter::new(
            None,
            Some(vec![zorai_protocol::tool_names::BASH_COMMAND.into()]),
        )
        .unwrap();
        let reason = filter
            .deny_reason(zorai_protocol::tool_names::BASH_COMMAND)
            .unwrap();
        assert!(reason.contains("blacklisted"));
    }
}
