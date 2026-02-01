use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillConfig {
    pub enabled: bool,
    pub personal_dir: Option<PathBuf>,
    pub project_dirs: Vec<PathBuf>,
    pub auto_apply: bool,
}
