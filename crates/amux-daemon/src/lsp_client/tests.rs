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
    assert_eq!(matches[0].line, 10);
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
    let results = find_symbols("/tmp/nonexistent_workspace_lsp_test_1234", "foo", 10);
    assert!(results.is_empty());
}
