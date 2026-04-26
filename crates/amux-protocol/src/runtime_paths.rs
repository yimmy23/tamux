use std::path::{Path, PathBuf};

pub fn tamux_root_dir() -> PathBuf {
    canonical_tamux_root_dir_from_parts(
        cfg!(windows),
        dirs::home_dir().as_deref(),
        dirs::data_local_dir().as_deref(),
    )
}

pub fn tamux_skills_dir() -> PathBuf {
    canonical_tamux_skills_dir_from_parts(
        cfg!(windows),
        dirs::home_dir().as_deref(),
        dirs::data_local_dir().as_deref(),
    )
}

pub fn tamux_guidelines_dir() -> PathBuf {
    canonical_tamux_guidelines_dir_from_parts(
        cfg!(windows),
        dirs::home_dir().as_deref(),
        dirs::data_local_dir().as_deref(),
    )
}

pub fn legacy_agent_skills_dir(agent_data_dir: &Path) -> PathBuf {
    agent_data_dir.join("skills")
}

pub fn thread_root_dir(root: &Path, thread_id: &str) -> PathBuf {
    root.join("threads")
        .join(sanitize_runtime_path_segment(thread_id))
}

pub fn thread_artifacts_dir(root: &Path, thread_id: &str) -> PathBuf {
    thread_root_dir(root, thread_id).join("artifacts")
}

pub fn thread_specs_dir(root: &Path, thread_id: &str) -> PathBuf {
    thread_artifacts_dir(root, thread_id).join("specs")
}

pub fn thread_media_dir(root: &Path, thread_id: &str) -> PathBuf {
    thread_artifacts_dir(root, thread_id).join("media")
}

pub fn thread_previews_dir(root: &Path, thread_id: &str) -> PathBuf {
    thread_artifacts_dir(root, thread_id).join("previews")
}

fn canonical_tamux_root_dir_from_parts(
    is_windows: bool,
    home_dir: Option<&Path>,
    local_data_dir: Option<&Path>,
) -> PathBuf {
    if is_windows {
        local_data_dir
            .map(Path::to_path_buf)
            .or_else(|| home_dir.map(|home| home.join("AppData").join("Local")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tamux")
    } else {
        home_dir.unwrap_or_else(|| Path::new(".")).join(".tamux")
    }
}

fn canonical_tamux_skills_dir_from_parts(
    is_windows: bool,
    home_dir: Option<&Path>,
    local_data_dir: Option<&Path>,
) -> PathBuf {
    canonical_tamux_root_dir_from_parts(is_windows, home_dir, local_data_dir).join("skills")
}

fn canonical_tamux_guidelines_dir_from_parts(
    is_windows: bool,
    home_dir: Option<&Path>,
    local_data_dir: Option<&Path>,
) -> PathBuf {
    canonical_tamux_root_dir_from_parts(is_windows, home_dir, local_data_dir).join("guidelines")
}

fn sanitize_runtime_path_segment(raw: &str) -> String {
    raw.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_tamux_root_dir_uses_home_dir_on_unix() {
        assert_eq!(
            canonical_tamux_root_dir_from_parts(false, Some(Path::new("/home/aline")), None),
            PathBuf::from("/home/aline/.tamux")
        );
    }

    #[test]
    fn canonical_tamux_root_dir_uses_local_app_data_on_windows() {
        assert_eq!(
            canonical_tamux_root_dir_from_parts(
                true,
                Some(Path::new(r"C:\Users\aline")),
                Some(Path::new(r"C:\Users\aline\AppData\Local"))
            ),
            PathBuf::from(r"C:\Users\aline\AppData\Local").join("tamux")
        );
    }

    #[test]
    fn canonical_skill_root_is_nested_under_tamux_root() {
        let root = canonical_tamux_root_dir_from_parts(false, Some(Path::new("/tmp/tamux")), None);
        assert_eq!(
            canonical_tamux_skills_dir_from_parts(false, Some(Path::new("/tmp/tamux")), None),
            root.join("skills")
        );
    }

    #[test]
    fn canonical_guidelines_root_is_nested_under_tamux_root() {
        let root = canonical_tamux_root_dir_from_parts(false, Some(Path::new("/tmp/tamux")), None);
        assert_eq!(
            canonical_tamux_guidelines_dir_from_parts(false, Some(Path::new("/tmp/tamux")), None),
            root.join("guidelines")
        );
    }

    #[test]
    fn legacy_agent_skills_dir_requires_explicit_migration_helper() {
        let agent_data_dir = Path::new("/tmp/tamux/agent");
        assert_eq!(
            legacy_agent_skills_dir(agent_data_dir),
            PathBuf::from("/tmp/tamux/agent/skills")
        );
    }

    #[test]
    fn thread_artifact_dirs_live_under_threads_subtree() {
        let root = Path::new("/home/aline/.tamux");
        assert_eq!(
            thread_root_dir(root, "thread-123"),
            PathBuf::from("/home/aline/.tamux/threads/thread-123")
        );
        assert_eq!(
            thread_artifacts_dir(root, "thread-123"),
            PathBuf::from("/home/aline/.tamux/threads/thread-123/artifacts")
        );
        assert_eq!(
            thread_specs_dir(root, "thread-123"),
            PathBuf::from("/home/aline/.tamux/threads/thread-123/artifacts/specs")
        );
        assert_eq!(
            thread_media_dir(root, "thread-123"),
            PathBuf::from("/home/aline/.tamux/threads/thread-123/artifacts/media")
        );
        assert_eq!(
            thread_previews_dir(root, "thread-123"),
            PathBuf::from("/home/aline/.tamux/threads/thread-123/artifacts/previews")
        );
    }

    #[test]
    fn thread_artifact_dirs_sanitize_path_segments() {
        let root = Path::new("/home/aline/.tamux");
        assert_eq!(
            thread_root_dir(root, "../thread bad"),
            PathBuf::from("/home/aline/.tamux/threads/___thread_bad")
        );
    }
}
