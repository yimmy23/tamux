fn is_code_like_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file_name = lower
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(lower.as_str());
    if matches!(file_name, "dockerfile" | "makefile") {
        return true;
    }
    let Some(ext) = file_name.rsplit('.').next() else {
        return false;
    };
    matches!(
        ext,
        "rs" | "py"
            | "json"
            | "jsonl"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "toml"
            | "yaml"
            | "yml"
            | "xml"
            | "html"
            | "css"
            | "scss"
            | "sql"
            | "go"
            | "java"
            | "kt"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "hpp"
            | "cs"
            | "rb"
            | "php"
            | "swift"
            | "lua"
            | "r"
            | "pl"
    )
}

fn is_code_keyword(token: &str) -> bool {
    matches!(
        token,
        "as" | "async"
            | "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "def"
            | "else"
            | "enum"
            | "export"
            | "false"
            | "fn"
            | "for"
            | "from"
            | "function"
            | "if"
            | "impl"
            | "import"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "mut"
            | "null"
            | "pub"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "throw"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "use"
            | "var"
            | "while"
            | "yield"
    )
}

fn push_token_span(spans: &mut Vec<Span<'static>>, text: String, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.style == style {
            last.content.to_mut().push_str(&text);
            return;
        }
    }
    spans.push(Span::styled(text, style));
}

fn syntax_highlight_line(text: String, theme: &ThemeTokens) -> Line<'static> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            let start = index;
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }
            push_token_span(
                &mut spans,
                chars[start..index].iter().collect(),
                theme.fg_dim,
            );
            continue;
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < chars.len() {
                let current = chars[index];
                index += 1;
                if escaped {
                    escaped = false;
                    continue;
                }
                if current == '\\' {
                    escaped = true;
                } else if current == quote {
                    break;
                }
            }
            push_token_span(
                &mut spans,
                chars[start..index].iter().collect(),
                theme.accent_success,
            );
            continue;
        }

        if ch == '/' && chars.get(index + 1) == Some(&'/') {
            push_token_span(
                &mut spans,
                chars[index..].iter().collect(),
                theme.fg_dim.add_modifier(Modifier::ITALIC),
            );
            break;
        }
        if ch == '#' {
            push_token_span(
                &mut spans,
                chars[index..].iter().collect(),
                theme.fg_dim.add_modifier(Modifier::ITALIC),
            );
            break;
        }

        if ch.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric()
                    || matches!(chars[index], '.' | '_' | '+' | '-'))
            {
                index += 1;
            }
            push_token_span(
                &mut spans,
                chars[start..index].iter().collect(),
                theme.accent_secondary,
            );
            continue;
        }

        if ch == '_' || ch.is_ascii_alphabetic() {
            let start = index;
            index += 1;
            while index < chars.len() && (chars[index] == '_' || chars[index].is_ascii_alphanumeric())
            {
                index += 1;
            }
            let token = chars[start..index].iter().collect::<String>();
            let style = if is_code_keyword(&token) {
                theme.accent_primary
            } else {
                theme.fg_active
            };
            push_token_span(&mut spans, token, style);
            continue;
        }

        let style = theme.fg_dim;
        push_token_span(&mut spans, ch.to_string(), style);
        index += 1;
    }

    Line::from(spans)
}

fn push_syntax_highlighted(
    lines: &mut Vec<Line<'static>>,
    content: &str,
    width: usize,
    theme: &ThemeTokens,
) {
    for line in wrap_text(content, width.max(1)) {
        lines.push(syntax_highlight_line(line, theme));
    }
}
