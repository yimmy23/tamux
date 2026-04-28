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
        assert!(filter.is_allowed("bash_command"));
        assert!(filter.is_allowed("read_file"));
        assert!(!filter.has_restrictions());
    }

    #[test]
    fn deny_all_blocks_everything() {
        let filter = ToolFilter::deny_all();
        assert!(!filter.is_allowed("bash_command"));
        assert!(!filter.is_allowed("read_file"));
        assert!(filter.has_restrictions());
        assert_eq!(
            filter.deny_reason("bash_command").as_deref(),
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
            Some(vec!["bash_command".into(), "read_file".into()]),
            Some(vec!["read_file".into(), "write_file".into()]),
        );
        let err = result.unwrap_err();
        assert_eq!(err.conflicting_tools, vec!["read_file"]);
    }

    #[test]
    fn no_conflict_when_lists_are_disjoint() {
        let filter = ToolFilter::new(
            Some(vec!["bash_command".into()]),
            Some(vec!["write_file".into()]),
        );
        assert!(filter.is_ok());
    }

    // --- Whitelist-only ---

    #[test]
    fn whitelist_allows_listed_tools() {
        let filter =
            ToolFilter::new(Some(vec!["bash_command".into(), "read_file".into()]), None).unwrap();
        assert!(filter.is_allowed("bash_command"));
        assert!(filter.is_allowed("read_file"));
        assert!(filter.has_restrictions());
    }

    #[test]
    fn whitelist_blocks_unlisted_tools() {
        let filter = ToolFilter::new(Some(vec!["bash_command".into()]), None).unwrap();
        assert!(!filter.is_allowed("write_file"));
        assert!(!filter.is_allowed("search_files"));
    }

    // --- Blacklist-only ---

    #[test]
    fn blacklist_blocks_listed_tools() {
        let filter =
            ToolFilter::new(None, Some(vec!["bash_command".into(), "write_file".into()])).unwrap();
        assert!(!filter.is_allowed("bash_command"));
        assert!(!filter.is_allowed("write_file"));
    }

    #[test]
    fn blacklist_allows_unlisted_tools() {
        let filter = ToolFilter::new(None, Some(vec!["bash_command".into()])).unwrap();
        assert!(filter.is_allowed("read_file"));
        assert!(filter.is_allowed("search_files"));
    }

    // --- Combined ---

    #[test]
    fn combined_whitelist_and_blacklist() {
        let filter = ToolFilter::new(
            Some(vec![
                "bash_command".into(),
                "read_file".into(),
                "list_files".into(),
            ]),
            Some(vec!["bash_command".into()]),
        );
        // bash_command is in both → conflict
        assert!(filter.is_err());
    }

    #[test]
    fn combined_disjoint_filters_correctly() {
        let filter = ToolFilter::new(
            Some(vec!["read_file".into(), "list_files".into()]),
            Some(vec!["write_file".into()]),
        )
        .unwrap();
        assert!(filter.is_allowed("read_file"));
        assert!(filter.is_allowed("list_files"));
        // Not in whitelist:
        assert!(!filter.is_allowed("bash_command"));
        // In blacklist (but also not in whitelist, so blocked by whitelist first):
        assert!(!filter.is_allowed("write_file"));
    }

    // --- filtered_tools ---

    #[test]
    fn filtered_tools_returns_only_allowed() {
        let filter =
            ToolFilter::new(Some(vec!["read_file".into(), "list_files".into()]), None).unwrap();
        let all_tools = vec![
            make_tool("read_file"),
            make_tool("write_file"),
            make_tool("list_files"),
            make_tool("bash_command"),
        ];
        let filtered = filter.filtered_tools(all_tools);
        let names: Vec<&str> = filtered.iter().map(|t| t.function.name.as_str()).collect();
        assert_eq!(names, vec!["read_file", "list_files"]);
    }

    #[test]
    fn filtered_tools_returns_all_when_no_restrictions() {
        let filter = ToolFilter::allow_all();
        let tools = vec![make_tool("a"), make_tool("b"), make_tool("c")];
        let filtered = filter.filtered_tools(tools);
        assert_eq!(filtered.len(), 3);
    }

    // --- deny_reason ---

    #[test]
    fn deny_reason_none_for_allowed_tool() {
        let filter = ToolFilter::new(Some(vec!["bash_command".into()]), None).unwrap();
        assert!(filter.deny_reason("bash_command").is_none());
    }

    #[test]
    fn deny_reason_whitelist_explains_missing() {
        let filter = ToolFilter::new(Some(vec!["bash_command".into()]), None).unwrap();
        let reason = filter.deny_reason("write_file").unwrap();
        assert!(reason.contains("not in the whitelist"));
        assert!(reason.contains("bash_command"));
    }

    #[test]
    fn deny_reason_blacklist_explains_blocked() {
        let filter = ToolFilter::new(None, Some(vec!["bash_command".into()])).unwrap();
        let reason = filter.deny_reason("bash_command").unwrap();
        assert!(reason.contains("blacklisted"));
    }
}
