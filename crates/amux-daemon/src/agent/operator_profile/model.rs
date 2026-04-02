//! Operator profile domain model.
//!
//! Defines the core value + spec types used by the interview planner and
//! profile snapshot. All types are plain data — no async, no I/O.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Field value
// ---------------------------------------------------------------------------

/// A stored profile field value with provenance metadata.
#[derive(Debug, Clone)]
pub struct ProfileFieldValue {
    /// JSON-encoded field value (string, bool, number, array…).
    pub value_json: String,
    /// Confidence in [0.0, 1.0]. 1.0 = explicit operator answer.
    pub confidence: f64,
    /// How the value was obtained: `"explicit_answer"`, `"inference"`, `"observation"`, etc.
    pub source: String,
    /// Unix-epoch millisecond timestamp of the last update.
    pub updated_at: u64,
}

// ---------------------------------------------------------------------------
// Field specification
// ---------------------------------------------------------------------------

/// Input widget hint for profile questions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Bool,
    Select,
}

impl InputKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            InputKind::Text => "text",
            InputKind::Bool => "bool",
            InputKind::Select => "select",
        }
    }
}

/// Specification for a single profile field/question.
#[derive(Debug, Clone)]
pub struct ProfileFieldSpec {
    /// Dotted key matching the field row in the database.
    pub field_key: String,
    /// Human-readable question text shown to the operator.
    pub prompt: String,
    /// Input kind hint: `"text"`, `"bool"`, or `"select"`.
    pub input_kind: String,
    /// Whether this field must be answered before the session is considered complete.
    pub required: bool,
}

// ---------------------------------------------------------------------------
// Profile snapshot
// ---------------------------------------------------------------------------

/// Full snapshot of operator profile fields indexed by field key.
#[derive(Debug, Clone, Default)]
pub struct OperatorProfileSnapshot {
    pub fields: HashMap<String, ProfileFieldValue>,
}

impl OperatorProfileSnapshot {
    /// Build a snapshot from history store rows.
    pub fn from_rows(rows: &[crate::history::OperatorProfileFieldRow]) -> Self {
        let mut fields = HashMap::new();
        for row in rows {
            fields.insert(
                row.field_key.clone(),
                ProfileFieldValue {
                    value_json: row.field_value_json.clone(),
                    confidence: row.confidence,
                    source: row.source.clone(),
                    updated_at: row.updated_at as u64,
                },
            );
        }
        Self { fields }
    }

    /// Serialise the snapshot to a JSON object suitable for IPC responses.
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (key, val) in &self.fields {
            let parsed = serde_json::from_str::<serde_json::Value>(&val.value_json)
                .unwrap_or(serde_json::Value::Null);
            let entry = serde_json::json!({
                "value": parsed,
                "confidence": val.confidence,
                "source": val.source,
                "updated_at": val.updated_at,
            });
            map.insert(key.clone(), entry);
        }
        serde_json::Value::Object(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_spec(key: &str, required: bool) -> ProfileFieldSpec {
        ProfileFieldSpec {
            field_key: key.to_string(),
            prompt: format!("Question for {key}"),
            input_kind: "text".to_string(),
            required,
        }
    }

    #[test]
    fn input_kind_str_roundtrip() {
        assert_eq!(InputKind::Text.as_str(), "text");
        assert_eq!(InputKind::Bool.as_str(), "bool");
        assert_eq!(InputKind::Select.as_str(), "select");
    }

    #[test]
    fn profile_field_spec_stores_fields() {
        let spec = make_spec("my_field", true);
        assert_eq!(spec.field_key, "my_field");
        assert!(spec.required);
        assert_eq!(spec.input_kind, "text");
    }

    #[test]
    fn snapshot_to_json_encodes_values() {
        let mut snap = OperatorProfileSnapshot::default();
        snap.fields.insert(
            "name".to_string(),
            ProfileFieldValue {
                value_json: "\"Alice\"".to_string(),
                confidence: 1.0,
                source: "explicit_answer".to_string(),
                updated_at: 42,
            },
        );
        let json = snap.to_json();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert_eq!(obj["name"]["value"], "Alice");
        assert_eq!(obj["name"]["confidence"], 1.0);
    }

    #[test]
    fn snapshot_to_json_handles_invalid_json_gracefully() {
        let mut snap = OperatorProfileSnapshot::default();
        snap.fields.insert(
            "bad".to_string(),
            ProfileFieldValue {
                value_json: "NOT_VALID_JSON{{{".to_string(),
                confidence: 0.5,
                source: "inference".to_string(),
                updated_at: 0,
            },
        );
        let json = snap.to_json();
        let obj = json.as_object().unwrap();
        assert_eq!(obj["bad"]["value"], serde_json::Value::Null);
    }
}
