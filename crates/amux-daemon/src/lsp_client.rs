//! Lightweight LSP client for workspace symbol search.
//!
//! Communicates with language servers over JSON-RPC / stdio using the standard
//! Content-Length framing defined in the LSP specification.  The client
//! auto-detects which language servers are available on `$PATH` and falls back
//! gracefully (returning an empty `Vec`) when none are found.

use amux_protocol::SymbolMatch;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};

/// Supported language server binaries and the file extensions they handle.
const KNOWN_SERVERS: &[(&str, &[&str])] = &[
    ("rust-analyzer", &["rs"]),
    ("typescript-language-server", &["ts", "tsx", "js", "jsx"]),
    ("pyright-langserver", &["py"]),
];

/// Maps an LSP `SymbolKind` integer to a human-readable string.
fn symbol_kind_name(kind: u64) -> &'static str {
    match kind {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type_parameter",
        _ => "symbol",
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC framing helpers
// ---------------------------------------------------------------------------

static REQUEST_ID: AtomicI64 = AtomicI64::new(1);

fn next_id() -> i64 {
    REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// Encode a JSON-RPC message with `Content-Length` header framing.
fn encode_message(msg: &Value) -> Vec<u8> {
    let body = serde_json::to_string(msg).expect("failed to serialize JSON-RPC message");
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
}

/// Read a single LSP message from a buffered reader.
///
/// Returns `None` on EOF or framing error.
fn read_message(reader: &mut BufReader<impl std::io::Read>) -> Option<Value> {
    // Read headers until we find an empty line.
    let mut content_length: Option<usize> = None;
    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).ok()? == 0 {
            return None; // EOF
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse().ok();
        }
    }

    let length = content_length?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

// ---------------------------------------------------------------------------
// LspClient
// ---------------------------------------------------------------------------

/// A thin wrapper around a language-server child process.
pub struct LspClient {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl LspClient {
    /// Start a language server process.
    ///
    /// Returns `None` if the binary is not found or fails to launch.
    fn start(binary: &str, workspace_root: &str) -> Option<Self> {
        // Verify the binary exists on PATH.
        let status = Command::new("which")
            .arg(binary)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }

        let mut args: Vec<&str> = Vec::new();
        // typescript-language-server requires --stdio flag.
        if binary == "typescript-language-server" {
            args.push("--stdio");
        }
        // pyright-langserver requires --stdio flag.
        if binary == "pyright-langserver" {
            args.push("--stdio");
        }

        let mut child = Command::new(binary)
            .args(&args)
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdout = child.stdout.take()?;
        let reader = BufReader::new(stdout);

        Some(LspClient { child, reader })
    }

    /// Send a JSON-RPC request and return the response `result` field.
    fn request(&mut self, method: &str, params: Value) -> Option<Value> {
        let id = next_id();
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let stdin = self.child.stdin.as_mut()?;
        let encoded = encode_message(&msg);
        stdin.write_all(&encoded).ok()?;
        stdin.flush().ok()?;

        // Read responses until we find our matching id.
        // Language servers may send notifications or other messages interleaved.
        for _ in 0..200 {
            let response = read_message(&mut self.reader)?;
            if response.get("id").and_then(|v| v.as_i64()) == Some(id) {
                return response.get("result").cloned();
            }
            // If we get an error response for our id, bail.
            if response.get("id").and_then(|v| v.as_i64()) == Some(id)
                && response.get("error").is_some()
            {
                return None;
            }
        }
        None
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    fn notify(&mut self, method: &str, params: Value) -> Option<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let stdin = self.child.stdin.as_mut()?;
        let encoded = encode_message(&msg);
        stdin.write_all(&encoded).ok()?;
        stdin.flush().ok()?;
        Some(())
    }

    /// Perform the LSP initialize / initialized handshake.
    fn initialize(&mut self, workspace_root: &str) -> Option<()> {
        let root_uri = format!(
            "file://{}",
            Path::new(workspace_root).canonicalize().ok()?.display()
        );

        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "workspace": {
                    "symbol": {
                        "dynamicRegistration": false
                    }
                }
            },
            "workspaceFolders": [
                {
                    "uri": root_uri,
                    "name": Path::new(workspace_root)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("workspace")
                }
            ]
        });

        let _init_result = self.request("initialize", params)?;

        // Send the initialized notification.
        self.notify("initialized", json!({}))?;

        Some(())
    }

    /// Send `workspace/symbol` and return the raw JSON response.
    fn workspace_symbol(&mut self, query: &str) -> Option<Value> {
        let params = json!({ "query": query });
        self.request("workspace/symbol", params)
    }

    /// Send `shutdown` + `exit` to gracefully terminate the server.
    fn shutdown(&mut self) {
        // shutdown is a request – wait for the response.
        let _ = self.request("shutdown", json!(null));
        // exit is a notification.
        let _ = self.notify("exit", json!(null));
        // Give the process a moment then reap it.
        let _ = self.child.wait();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Best-effort cleanup: kill if still running.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect which language servers are available on `$PATH` for files under
/// `workspace_root`.
fn detect_servers(workspace_root: &str) -> Vec<&'static str> {
    // First, figure out which file extensions are present in the workspace
    // so we only launch relevant servers.  We do a shallow scan (max depth 4)
    // to keep this fast.
    let mut extensions_present: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for entry in walkdir::WalkDir::new(workspace_root)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
            extensions_present.insert(ext.to_lowercase());
        }
    }

    let mut servers = Vec::new();
    for &(binary, exts) in KNOWN_SERVERS {
        let relevant = exts.iter().any(|ext| extensions_present.contains(*ext));
        if !relevant {
            continue;
        }
        // Check if the binary exists on PATH.
        let found = Command::new("which")
            .arg(binary)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if found {
            servers.push(binary);
        }
    }
    servers
}

/// Parse LSP `workspace/symbol` results into `SymbolMatch` entries.
fn parse_symbol_results(result: &Value, limit: usize) -> Vec<SymbolMatch> {
    let items = match result.as_array() {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    let mut matches = Vec::with_capacity(items.len().min(limit));

    for item in items {
        if matches.len() >= limit {
            break;
        }

        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let kind_num = item.get("kind").and_then(|v| v.as_u64()).unwrap_or(0);
        let kind = symbol_kind_name(kind_num);

        // LSP SymbolInformation has a `location` field.
        let location = item.get("location");
        let (path, line) = if let Some(loc) = location {
            let uri = loc.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            let path = uri.strip_prefix("file://").unwrap_or(uri);
            let line = loc
                .get("range")
                .and_then(|r| r.get("start"))
                .and_then(|s| s.get("line"))
                .and_then(|l| l.as_u64())
                .map(|l| (l + 1) as usize) // LSP lines are 0-indexed
                .unwrap_or(1);
            (path.to_string(), line)
        } else {
            // WorkspaceSymbol may use `location` with just a uri (no range).
            let uri = item
                .get("location")
                .and_then(|l| l.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let path = uri.strip_prefix("file://").unwrap_or(uri);
            (path.to_string(), 1)
        };

        // Build a snippet from the container name and symbol name.
        let container = item
            .get("containerName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let snippet = if container.is_empty() {
            format!("{kind} {name}")
        } else {
            format!("{kind} {container}::{name}")
        };

        matches.push(SymbolMatch {
            path,
            line,
            kind: kind.to_string(),
            snippet,
        });
    }

    matches
}

/// Query language servers for workspace symbols matching `query`.
///
/// Tries each available language server in turn and aggregates results.
/// Returns an empty `Vec` if no language servers are found or all fail --
/// the caller can then fall back to regex-based search.
pub fn find_symbols(workspace_root: &str, query: &str, limit: usize) -> Vec<SymbolMatch> {
    let servers = detect_servers(workspace_root);
    if servers.is_empty() {
        tracing::debug!("no language servers detected on PATH");
        return Vec::new();
    }

    let mut all_matches: Vec<SymbolMatch> = Vec::new();

    for binary in servers {
        if all_matches.len() >= limit {
            break;
        }

        tracing::debug!(server = binary, "attempting LSP workspace/symbol query");

        let mut client = match LspClient::start(binary, workspace_root) {
            Some(c) => c,
            None => {
                tracing::debug!(server = binary, "failed to start language server");
                continue;
            }
        };

        if client.initialize(workspace_root).is_none() {
            tracing::debug!(server = binary, "LSP initialize handshake failed");
            client.shutdown();
            continue;
        }

        let remaining = limit.saturating_sub(all_matches.len());
        match client.workspace_symbol(query) {
            Some(result) => {
                let symbols = parse_symbol_results(&result, remaining);
                tracing::debug!(
                    server = binary,
                    count = symbols.len(),
                    "received workspace/symbol results"
                );
                all_matches.extend(symbols);
            }
            None => {
                tracing::debug!(server = binary, "workspace/symbol request failed");
            }
        }

        client.shutdown();
    }

    // Deduplicate by (path, line).
    let mut seen: HashMap<(String, usize), bool> = HashMap::new();
    all_matches.retain(|m| seen.insert((m.path.clone(), m.line), true).is_none());

    all_matches.truncate(limit);
    all_matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_name() {
        assert_eq!(symbol_kind_name(12), "function");
        assert_eq!(symbol_kind_name(5), "class");
        assert_eq!(symbol_kind_name(23), "struct");
        assert_eq!(symbol_kind_name(999), "symbol");
    }

    #[test]
    fn test_parse_empty_result() {
        let result = json!([]);
        let matches = parse_symbol_results(&result, 10);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_parse_null_result() {
        let result = json!(null);
        let matches = parse_symbol_results(&result, 10);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_parse_symbol_results() {
        let result = json!([
            {
                "name": "MyStruct",
                "kind": 23,
                "location": {
                    "uri": "file:///home/user/project/src/main.rs",
                    "range": {
                        "start": { "line": 9, "character": 0 },
                        "end": { "line": 9, "character": 20 }
                    }
                },
                "containerName": "my_module"
            },
            {
                "name": "do_stuff",
                "kind": 12,
                "location": {
                    "uri": "file:///home/user/project/src/lib.rs",
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 15 }
                    }
                }
            }
        ]);

        let matches = parse_symbol_results(&result, 10);
        assert_eq!(matches.len(), 2);

        assert_eq!(matches[0].path, "/home/user/project/src/main.rs");
        assert_eq!(matches[0].line, 10); // 0-indexed -> 1-indexed
        assert_eq!(matches[0].kind, "struct");
        assert_eq!(matches[0].snippet, "struct my_module::MyStruct");

        assert_eq!(matches[1].path, "/home/user/project/src/lib.rs");
        assert_eq!(matches[1].line, 1);
        assert_eq!(matches[1].kind, "function");
        assert_eq!(matches[1].snippet, "function do_stuff");
    }

    #[test]
    fn test_parse_symbol_results_respects_limit() {
        let result = json!([
            {
                "name": "A",
                "kind": 12,
                "location": {
                    "uri": "file:///a.rs",
                    "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } }
                }
            },
            {
                "name": "B",
                "kind": 12,
                "location": {
                    "uri": "file:///b.rs",
                    "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } }
                }
            },
            {
                "name": "C",
                "kind": 12,
                "location": {
                    "uri": "file:///c.rs",
                    "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } }
                }
            }
        ]);

        let matches = parse_symbol_results(&result, 2);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_encode_message_framing() {
        let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "test"});
        let encoded = encode_message(&msg);
        let encoded_str = String::from_utf8(encoded).unwrap();
        assert!(encoded_str.starts_with("Content-Length: "));
        assert!(encoded_str.contains("\r\n\r\n"));
    }

    #[test]
    fn test_find_symbols_no_servers() {
        // With a non-existent workspace, no servers should be detected.
        let results = find_symbols("/tmp/nonexistent_workspace_lsp_test_1234", "foo", 10);
        assert!(results.is_empty());
    }
}
