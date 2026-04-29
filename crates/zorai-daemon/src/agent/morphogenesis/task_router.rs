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
    if lowercase.contains("medical")
        || lowercase.contains("clinical")
        || lowercase.contains("patient")
        || lowercase.contains("symptom")
        || lowercase.contains("treatment")
        || lowercase.contains("diagnosis")
    {
        domains.insert("medical".to_string());
        domains.insert("clinical-guidelines".to_string());
    }
    if lowercase.contains("finance")
        || lowercase.contains("financial")
        || lowercase.contains("budget")
        || lowercase.contains("forecast")
        || lowercase.contains("portfolio")
        || lowercase.contains("valuation")
        || lowercase.contains("investment")
    {
        domains.insert("finance".to_string());
        domains.insert("risk-analysis".to_string());
    }
    if lowercase.contains("legal")
        || lowercase.contains("contract")
        || lowercase.contains("clause")
        || lowercase.contains("law")
        || lowercase.contains("regulation")
        || lowercase.contains("compliance")
    {
        domains.insert("legal".to_string());
        domains.insert("issue-spotting".to_string());
    }
    if lowercase.contains("art")
        || lowercase.contains("artistic")
        || lowercase.contains("design direction")
        || lowercase.contains("visual")
        || lowercase.contains("brand")
        || lowercase.contains("moodboard")
        || lowercase.contains("illustration")
    {
        domains.insert("art-direction".to_string());
        domains.insert("concept-development".to_string());
    }
    if lowercase.contains("scientific")
        || lowercase.contains("study")
        || lowercase.contains("paper")
        || lowercase.contains("experiment")
        || lowercase.contains("methodology")
        || lowercase.contains("statistical")
        || lowercase.contains("peer review")
    {
        domains.insert("science".to_string());
        domains.insert("methodology".to_string());
    }
    if lowercase.contains("marketing")
        || lowercase.contains("positioning")
        || lowercase.contains("messaging")
        || lowercase.contains("go-to-market")
        || lowercase.contains("gtm")
        || lowercase.contains("launch")
        || lowercase.contains("audience")
    {
        domains.insert("marketing".to_string());
        domains.insert("positioning".to_string());
    }

    if domains.is_empty() {
        domains.insert("general".to_string());
    }

    domains.into_iter().collect()
}
