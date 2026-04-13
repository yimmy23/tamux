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

pub fn legacy_agent_skills_dir(agent_data_dir: &Path) -> PathBuf {
    agent_data_dir.join("skills")
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
    fn legacy_agent_skills_dir_requires_explicit_migration_helper() {
        let agent_data_dir = Path::new("/tmp/tamux/agent");
        assert_eq!(
            legacy_agent_skills_dir(agent_data_dir),
            PathBuf::from("/tmp/tamux/agent/skills")
        );
    }
}
