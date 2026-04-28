use regex::Regex;
use std::sync::LazyLock;

/// Patterns that match common sensitive data.
static PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // AWS access key IDs
        (
            Regex::new(r"(?i)(AKIA[0-9A-Z]{16})").unwrap(),
            "***AWS_KEY***",
        ),
        // AWS secret keys (40 hex/base64 chars after a prefix)
        (
            Regex::new(r"(?i)(aws_secret_access_key\s*=\s*)\S+").unwrap(),
            "${1}***REDACTED***",
        ),
        // GitHub personal access tokens (before generic patterns so they match first)
        (
            Regex::new(r"ghp_[A-Za-z0-9]{36}").unwrap(),
            "***GH_TOKEN***",
        ),
        // Generic API key patterns (key=..., token=..., secret=...)
        (
            Regex::new(r#"(?i)((?:api[_-]?key|token|secret|password|passwd|auth)\s*[=:]\s*)['"]?\S+['"]?"#).unwrap(),
            "${1}***REDACTED***",
        ),
        // Bearer tokens
        (
            Regex::new(r"(?i)(Bearer\s+)\S+").unwrap(),
            "${1}***REDACTED***",
        ),
        // Generic hex secrets (32+ chars of hex that look like keys)
        (
            Regex::new(r"\b[0-9a-f]{40,}\b").unwrap(),
            "***HEX_REDACTED***",
        ),
        // Private key markers
        (
            Regex::new(r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |DSA )?PRIVATE KEY-----").unwrap(),
            "***PRIVATE_KEY_REDACTED***",
        ),
    ]
});

/// Scrub sensitive data from a string using known patterns.
/// Returns the scrubbed version.
pub fn scrub_sensitive(text: &str) -> String {
    let mut result = text.to_string();
    for (pattern, replacement) in PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).into_owned();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_aws_key() {
        let input = "my key is AKIAIOSFODNN7EXAMPLE";
        let output = scrub_sensitive(input);
        assert!(output.contains("***AWS_KEY***"));
        assert!(!output.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn scrubs_bearer_token() {
        let input = "Authorization: Bearer super_secret_token_123";
        let output = scrub_sensitive(input);
        assert!(output.contains("***REDACTED***"));
        assert!(!output.contains("super_secret_token_123"));
    }

    #[test]
    fn scrubs_github_token() {
        // Standalone GitHub PAT (e.g. in a URL or log line)
        let input = "clone https://ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij@github.com/repo";
        let output = scrub_sensitive(input);
        assert!(output.contains("***GH_TOKEN***"));
        assert!(!output.contains("ghp_"));

        // When prefixed with `token=`, the generic key pattern also redacts it
        let input2 = "token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let output2 = scrub_sensitive(input2);
        assert!(!output2.contains("ghp_"), "GitHub token should be redacted");
    }
}
