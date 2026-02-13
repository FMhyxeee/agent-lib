use crate::error::{AgentError, AgentResult};
use crate::skills::definition::{Skill, SkillMetadata, SkillSource};
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct SkillParser;

impl SkillParser {
    /// 解析 SKILL.md 文件
    pub async fn parse_skill_file(path: &Path, source: SkillSource) -> AgentResult<Skill> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|err| AgentError::InvalidConfig(format!("读取技能文件失败: {err}")))?;

        let (metadata, body) = Self::parse_frontmatter(&content)?;

        let directory = path
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| AgentError::InvalidConfig("技能文件路径无父目录".to_string()))?;

        let auxiliary_files = Self::scan_auxiliary_files(&directory, path).await;

        Ok(Skill {
            metadata,
            content: body,
            path: path.to_path_buf(),
            directory,
            auxiliary_files,
            source,
        })
    }

    /// 解析 YAML frontmatter
    fn parse_frontmatter(content: &str) -> AgentResult<(SkillMetadata, String)> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err(AgentError::InvalidConfig(
                "技能文件缺少 YAML frontmatter".to_string(),
            ));
        }

        let mut lines = trimmed.lines();
        let first = lines.next().unwrap_or_default();
        if first.trim() != "---" {
            return Err(AgentError::InvalidConfig(
                "技能文件 frontmatter 格式错误".to_string(),
            ));
        }

        let mut frontmatter_lines = Vec::new();
        let mut body_lines = Vec::new();
        let mut in_frontmatter = true;

        for line in lines {
            if in_frontmatter {
                if line.trim() == "---" {
                    in_frontmatter = false;
                    continue;
                }
                frontmatter_lines.push(line);
            } else {
                body_lines.push(line);
            }
        }

        if in_frontmatter {
            return Err(AgentError::InvalidConfig(
                "技能文件 frontmatter 未正确结束".to_string(),
            ));
        }

        let frontmatter = frontmatter_lines.join("\n");
        let metadata: SkillMetadata = serde_yaml::from_str(&frontmatter).map_err(|err| {
            AgentError::InvalidConfig(format!("解析技能 frontmatter 失败: {err}"))
        })?;

        if metadata.name.trim().is_empty() {
            return Err(AgentError::InvalidConfig("技能名称不能为空".to_string()));
        }

        if metadata.description.trim().is_empty() {
            return Err(AgentError::InvalidConfig("技能描述不能为空".to_string()));
        }

        Ok((metadata, body_lines.join("\n")))
    }

    /// 扫描辅助文件
    async fn scan_auxiliary_files(dir: &Path, skill_file: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();

        let Ok(mut entries) = fs::read_dir(dir).await else {
            return files;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path == skill_file {
                continue;
            }
            if path.is_file() {
                files.push(path);
            }
        }

        files
    }
}
