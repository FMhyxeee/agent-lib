use crate::protocol::SkillEntry;
use crate::skills::definition::{Skill, SkillSource};
use std::collections::HashMap;

pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    sources: Vec<SkillSource>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            sources: Vec::new(),
        }
    }

    pub fn with_sources(sources: Vec<SkillSource>) -> Self {
        Self {
            skills: HashMap::new(),
            sources,
        }
    }

    pub fn register(&mut self, skill: Skill) {
        self.skills.insert(skill.metadata.name.clone(), skill);
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn list(&self) -> Vec<SkillEntry> {
        self.skills
            .values()
            .map(|skill| SkillEntry {
                name: skill.metadata.name.clone(),
                description: skill.metadata.description.clone(),
                path: skill.path.clone(),
                source: skill.source.as_label().to_string(),
                has_auxiliary_files: !skill.auxiliary_files.is_empty(),
            })
            .collect()
    }

    pub fn find_matching(&self, query: &str) -> Vec<&Skill> {
        let query_lower = query.to_lowercase();
        self.skills
            .values()
            .filter(|skill| {
                skill.metadata.name.to_lowercase().contains(&query_lower)
                    || skill
                        .metadata
                        .description
                        .to_lowercase()
                        .contains(&query_lower)
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.skills.clear();
    }

    pub fn sources(&self) -> &[SkillSource] {
        &self.sources
    }
}
