use crate::error::{AgentError, AgentResult};
use crate::skills::definition::{Skill, SkillSource};
use crate::skills::parser::SkillParser;
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct SkillLoader {
    sources: Vec<SkillSource>,
}

impl SkillLoader {
    pub fn new() -> Self {
        Self { sources: vec![] }
    }

    pub fn with_sources(sources: Vec<SkillSource>) -> Self {
        Self { sources }
    }

    pub async fn load_all(&self) -> AgentResult<Vec<Skill>> {
        let sources = if self.sources.is_empty() {
            vec![SkillSource::Personal, SkillSource::Project]
        } else {
            self.sources.clone()
        };

        let mut skills = Vec::new();
        for source in sources {
            let dirs = self.directories_for_source(&source)?;
            for dir in dirs {
                let mut loaded = self.load_from_directory(&dir, &source).await?;
                skills.append(&mut loaded);
            }
        }

        Ok(skills)
    }

    pub async fn load_from_directory(
        &self,
        dir: &Path,
        source: &SkillSource,
    ) -> AgentResult<Vec<Skill>> {
        let mut skills = Vec::new();
        if !dir.exists() {
            return Ok(skills);
        }

        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|err| AgentError::InvalidConfig(format!("读取技能目录失败: {err}")))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| AgentError::InvalidConfig(format!("读取技能目录失败: {err}")))?
        {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.is_file() {
                continue;
            }

            let skill = SkillParser::parse_skill_file(&skill_file, source.clone()).await?;
            skills.push(skill);
        }

        Ok(skills)
    }

    pub fn default_skill_directories() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        if let Some(home) = home_dir() {
            dirs.push(home.join(".cursor").join("skills"));
        }

        dirs.push(PathBuf::from(".cursor").join("skills"));

        dirs
    }

    fn directories_for_source(&self, source: &SkillSource) -> AgentResult<Vec<PathBuf>> {
        match source {
            SkillSource::Personal => {
                if let Some(home) = home_dir() {
                    Ok(vec![home.join(".cursor").join("skills")])
                } else {
                    Ok(Vec::new())
                }
            }
            SkillSource::Project => Ok(vec![PathBuf::from(".cursor").join("skills")]),
            SkillSource::Custom(path) => Ok(vec![path.clone()]),
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(home));
    }
    std::env::var("HOME").ok().map(PathBuf::from)
}
