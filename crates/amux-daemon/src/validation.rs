use amux_protocol::SymbolMatch;
use anyhow::Result;
use regex::Regex;
use std::sync::LazyLock;
use tree_sitter::Parser;
use walkdir::WalkDir;

use crate::lsp_client;

static SYMBOL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(fn|struct|enum|trait|impl|class|interface|type|const)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .unwrap()
});

pub fn validate_command(command: &str, language_hint: Option<&str>) -> Result<()> {
    let hint = language_hint.unwrap_or("shell");
    if matches!(hint, "shell" | "bash" | "sh") {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::language())
            .map_err(|_| anyhow::anyhow!("failed to initialize tree-sitter bash parser"))?;
        let tree = parser
            .parse(command, None)
            .ok_or_else(|| anyhow::anyhow!("tree-sitter did not return a parse tree"))?;
        if tree.root_node().has_error() {
            anyhow::bail!("tree-sitter rejected the shell payload");
        }
    }

    let trimmed = command.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty command payload");
    }

    Ok(())
}

pub fn find_symbol(workspace_root: &str, symbol: &str, limit: usize) -> Vec<SymbolMatch> {
    // Try LSP-based symbol search first; fall back to regex if no results.
    let lsp_results = lsp_client::find_symbols(workspace_root, symbol, limit);
    if !lsp_results.is_empty() {
        return lsp_results;
    }

    // Regex fallback: walk the workspace and grep for symbol definitions.
    let mut matches = Vec::new();
    for entry in WalkDir::new(workspace_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        if !matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "md") {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        for (index, line) in content.lines().enumerate() {
            let line_trimmed = line.trim();
            if !line_trimmed.contains(symbol) {
                continue;
            }

            let mut kind = "reference";
            if let Some(captures) = SYMBOL_PATTERN.captures(line_trimmed) {
                if captures.get(2).map(|value| value.as_str()) == Some(symbol) {
                    kind = captures
                        .get(1)
                        .map(|value| value.as_str())
                        .unwrap_or("symbol");
                }
            }

            matches.push(SymbolMatch {
                path: path.to_string_lossy().into_owned(),
                line: index + 1,
                kind: kind.to_string(),
                snippet: line_trimmed.to_string(),
            });

            if matches.len() >= limit {
                return matches;
            }
        }
    }

    matches
}
