use super::*;
use serde_yaml::Value;

#[derive(Debug, Default)]
struct ParsedSkillTagMetadata {
    explicit_context_tags: Vec<String>,
    name: Option<String>,
    description: Option<String>,
    summary: Option<String>,
    keywords: Vec<String>,
    triggers: Vec<String>,
    headings: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct TagSource<'a> {
    text: &'a str,
    weight: u32,
}

#[derive(Debug, Clone, Copy)]
struct TagRule {
    tag: &'static str,
    patterns: &'static [&'static str],
}

const TAG_SCORE_THRESHOLD: u32 = 3;

const TAG_RULES: &[TagRule] = &[
    TagRule {
        tag: "async",
        patterns: &["async", "async std", "futures", "tokio"],
    },
    TagRule {
        tag: "database",
        patterns: &[
            "database",
            "diesel",
            "mysql",
            "postgres",
            "postgresql",
            "prisma",
            "sequelize",
            "sqlite",
            "sqlx",
        ],
    },
    TagRule {
        tag: "desktop",
        patterns: &["desktop", "electron", "tauri"],
    },
    TagRule {
        tag: "docker",
        patterns: &["compose", "docker", "dockerfile"],
    },
    TagRule {
        tag: "frontend",
        patterns: &[
            "css", "frontend", "next", "react", "svelte", "tailwind", "vite", "vue",
        ],
    },
    TagRule {
        tag: "javascript",
        patterns: &["javascript", "js"],
    },
    TagRule {
        tag: "kubernetes",
        patterns: &["helm", "k8s", "kubectl", "kubernetes"],
    },
    TagRule {
        tag: "messaging",
        patterns: &["discord", "slack", "telegram"],
    },
    TagRule {
        tag: "node",
        patterns: &["bun", "node", "npm", "pnpm", "yarn"],
    },
    TagRule {
        tag: "python",
        patterns: &["pip", "pyproject", "pytest", "python"],
    },
    TagRule {
        tag: "rust",
        patterns: &["cargo", "cargo toml", "clippy", "rust", "rustc", "rustfmt"],
    },
    TagRule {
        tag: "terraform",
        patterns: &["hcl", "terraform", "tfstate"],
    },
    TagRule {
        tag: "terminal",
        patterns: &["cli", "shell", "terminal", "tui"],
    },
    TagRule {
        tag: "typescript",
        patterns: &["tsconfig", "tsx", "typescript"],
    },
    TagRule {
        tag: "wasm32",
        patterns: &[
            "wasm",
            "wasm bindgen",
            "wasm pack",
            "wasm32",
            "wasmtime",
            "webassembly",
        ],
    },
];

pub(super) fn infer_skill_tags(path: &str, content: &str, out: &mut BTreeSet<String>) {
    let metadata = parse_skill_tag_metadata(content);
    if !metadata.explicit_context_tags.is_empty() {
        out.extend(metadata.explicit_context_tags);
        return;
    }

    let normalized_path = normalize_tag_text(path);
    let normalized_name = metadata.name.as_deref().map(normalize_tag_text);
    let normalized_description = metadata.description.as_deref().map(normalize_tag_text);
    let normalized_summary = metadata.summary.as_deref().map(normalize_tag_text);
    let normalized_keywords = metadata
        .keywords
        .iter()
        .map(|value| normalize_tag_text(value))
        .collect::<Vec<_>>();
    let normalized_triggers = metadata
        .triggers
        .iter()
        .map(|value| normalize_tag_text(value))
        .collect::<Vec<_>>();
    let normalized_headings = metadata
        .headings
        .iter()
        .map(|value| normalize_tag_text(value))
        .collect::<Vec<_>>();

    let mut scores = BTreeMap::new();
    let mut sources = vec![TagSource {
        text: normalized_path.as_str(),
        weight: 3,
    }];
    if let Some(value) = normalized_name.as_deref() {
        sources.push(TagSource {
            text: value,
            weight: 3,
        });
    }
    if let Some(value) = normalized_description.as_deref() {
        sources.push(TagSource {
            text: value,
            weight: 1,
        });
    }
    if let Some(value) = normalized_summary.as_deref() {
        sources.push(TagSource {
            text: value,
            weight: 1,
        });
    }
    for value in &normalized_keywords {
        sources.push(TagSource {
            text: value.as_str(),
            weight: 3,
        });
    }
    for value in &normalized_triggers {
        sources.push(TagSource {
            text: value.as_str(),
            weight: 3,
        });
    }
    for value in &normalized_headings {
        sources.push(TagSource {
            text: value.as_str(),
            weight: 3,
        });
    }

    for source in sources {
        if source.text.trim().is_empty() {
            continue;
        }
        for rule in TAG_RULES {
            if rule
                .patterns
                .iter()
                .any(|pattern| contains_tag_pattern(source.text, pattern))
            {
                *scores.entry(rule.tag).or_insert(0) += source.weight;
            }
        }
    }

    for rule in TAG_RULES {
        if scores.get(rule.tag).copied().unwrap_or_default() >= TAG_SCORE_THRESHOLD {
            out.insert(rule.tag.to_string());
        }
    }
}

fn parse_skill_tag_metadata(content: &str) -> ParsedSkillTagMetadata {
    let (frontmatter, body) = split_frontmatter(content);
    let explicit_context_tags = collect_explicit_context_tags(frontmatter.as_ref());

    ParsedSkillTagMetadata {
        explicit_context_tags,
        name: extract_frontmatter_string(frontmatter.as_ref(), "name"),
        description: extract_frontmatter_string(frontmatter.as_ref(), "description"),
        summary: extract_frontmatter_string(frontmatter.as_ref(), "summary"),
        keywords: extract_frontmatter_list(frontmatter.as_ref(), "keywords"),
        triggers: {
            let mut triggers = extract_frontmatter_list(frontmatter.as_ref(), "triggers");
            triggers.extend(extract_trigger_section(body));
            dedupe_strings(&mut triggers);
            triggers
        },
        headings: extract_headings(body),
    }
}

fn collect_explicit_context_tags(frontmatter: Option<&Value>) -> Vec<String> {
    let mut tags = extract_frontmatter_list(frontmatter, "context_tags");
    tags.extend(extract_frontmatter_list(frontmatter, "tags"));
    tags.extend(extract_nested_frontmatter_list(
        frontmatter,
        &["metadata", "context_tags"],
    ));
    tags.extend(extract_nested_frontmatter_list(
        frontmatter,
        &["metadata", "tags"],
    ));
    tags.extend(extract_nested_frontmatter_list(
        frontmatter,
        &["zorai", "context_tags"],
    ));

    let mut deduped = BTreeSet::new();
    for tag in tags {
        let normalized = normalize_context_tag(&tag);
        if !normalized.is_empty() {
            deduped.insert(normalized);
        }
    }
    deduped.into_iter().collect()
}

fn split_frontmatter(content: &str) -> (Option<Value>, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return (None, content);
    };
    let Some(split_at) = rest.find("\n---\n") else {
        return (None, content);
    };

    let yaml = &rest[..split_at];
    let body = &rest[split_at + 5..];
    (serde_yaml::from_str::<Value>(yaml).ok(), body)
}

fn extract_frontmatter_string(frontmatter: Option<&Value>, key: &str) -> Option<String> {
    frontmatter
        .and_then(|value| value.as_mapping())
        .and_then(|mapping| mapping.get(Value::String(key.to_string())))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_frontmatter_list(frontmatter: Option<&Value>, key: &str) -> Vec<String> {
    frontmatter
        .and_then(|value| value.as_mapping())
        .and_then(|mapping| mapping.get(Value::String(key.to_string())))
        .map(value_to_string_list)
        .unwrap_or_default()
}

fn extract_nested_frontmatter_list(frontmatter: Option<&Value>, path: &[&str]) -> Vec<String> {
    let mut current = match frontmatter {
        Some(value) => value,
        None => return Vec::new(),
    };

    for key in path {
        let Some(mapping) = current.as_mapping() else {
            return Vec::new();
        };
        let Some(value) = mapping.get(Value::String((*key).to_string())) else {
            return Vec::new();
        };
        current = value;
    }

    value_to_string_list(current)
}

fn value_to_string_list(value: &Value) -> Vec<String> {
    match value {
        Value::String(item) => split_scalar_list(item),
        Value::Sequence(items) => items
            .iter()
            .filter_map(Value::as_str)
            .flat_map(split_scalar_list)
            .collect(),
        _ => Vec::new(),
    }
}

fn split_scalar_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn extract_headings(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let title = trimmed.trim_start_matches('#').trim();
            if trimmed.starts_with('#') && !title.is_empty() {
                Some(title.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn extract_trigger_section(body: &str) -> Vec<String> {
    let mut triggers = Vec::new();
    let mut in_triggers = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let heading = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
            in_triggers = heading == "triggers";
            continue;
        }
        if !in_triggers || trimmed.is_empty() {
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            triggers.push(item.trim().to_string());
            continue;
        }
        if let Some((_, item)) = trimmed.split_once(". ") {
            if trimmed
                .chars()
                .next()
                .map(|value| value.is_ascii_digit())
                .unwrap_or(false)
            {
                triggers.push(item.trim().to_string());
                continue;
            }
        }
        if !trimmed.starts_with('<') {
            triggers.push(trimmed.to_string());
        }
    }

    triggers
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut deduped = BTreeSet::new();
    for value in values.drain(..) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            deduped.insert(trimmed.to_string());
        }
    }
    values.extend(deduped);
}

fn normalize_tag_text(value: &str) -> String {
    let excerpt = excerpt_on_char_boundary(value, 4000);
    let mut normalized = String::with_capacity(excerpt.len() + 2);
    normalized.push(' ');
    let mut previous_was_space = true;

    for ch in excerpt.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    if !previous_was_space {
        normalized.push(' ');
    }

    normalized
}

fn contains_tag_pattern(haystack: &str, pattern: &str) -> bool {
    let normalized_pattern = normalize_tag_text(pattern);
    haystack.contains(normalized_pattern.as_str())
}

fn normalize_context_tag(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else if matches!(ch, '-' | '_') {
                '-'
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}
