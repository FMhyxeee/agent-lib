use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Skill 元数据（从 YAML frontmatter 解析）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

/// 完整的 Skill 结构
#[derive(Debug, Clone)]
pub struct Skill {
    pub metadata: SkillMetadata,
    pub content: String,
    pub path: PathBuf,
    pub directory: PathBuf,
    pub auxiliary_files: Vec<PathBuf>,
    pub source: SkillSource,
}

/// Skill 加载源类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    Personal,
    Project,
    Custom(PathBuf),
}

impl SkillSource {
    pub fn as_label(&self) -> &'static str {
        match self {
            SkillSource::Personal => "personal",
            SkillSource::Project => "project",
            SkillSource::Custom(_) => "custom",
        }
    }
}
