use std::collections::BTreeSet;

pub(crate) fn classify_domains(task_description: &str, capability_tags: &[String]) -> Vec<String> {
    let mut domains = BTreeSet::new();
    for tag in capability_tags {
        let trimmed = tag.trim().to_ascii_lowercase();
        if !trimmed.is_empty() {
            domains.insert(trimmed);
        }
    }

    let lowercase = task_description.to_ascii_lowercase();
    if lowercase.contains(".rs") || lowercase.contains("cargo") || lowercase.contains("rust") {
        domains.insert("rust".to_string());
    }
    if lowercase.contains(".py") || lowercase.contains("python") || lowercase.contains("pytest") {
        domains.insert("python".to_string());
    }
    if lowercase.contains(".ts")
        || lowercase.contains(".tsx")
        || lowercase.contains("typescript")
        || lowercase.contains("react")
        || lowercase.contains("npm")
    {
        domains.insert("typescript".to_string());
    }
    if lowercase.contains("investigate")
        || lowercase.contains("research")
        || lowercase.contains("analyze")
        || lowercase.contains("diagnose")
    {
        domains.insert("research".to_string());
    }
    if lowercase.contains("security") || lowercase.contains("audit") {
        domains.insert("security".to_string());
    }
    if lowercase.contains("test") || lowercase.contains("verify") {
        domains.insert("testing".to_string());
    }

    if domains.is_empty() {
        domains.insert("general".to_string());
    }

    domains.into_iter().collect()
}
