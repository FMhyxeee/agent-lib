use agent_lib::skills::{SkillLoader, SkillParser, SkillSource};
use std::fs;
use std::path::PathBuf;

fn create_temp_skill_dir() -> PathBuf {
    let base = std::env::temp_dir().join(format!("agent_lib_skill_test_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&base).expect("create temp dir");
    base
}

#[tokio::test]
async fn parse_skill_file_with_frontmatter() {
    let root = create_temp_skill_dir();
    let skill_dir = root.join("demo-skill");
    fs::create_dir_all(&skill_dir).expect("create skill dir");

    let content = r#"---
name: demo-skill
description: Demo skill description
---

# Demo Skill
Content body.
"#;

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(&skill_path, content).expect("write skill file");

    let skill = SkillParser::parse_skill_file(&skill_path, SkillSource::Custom(root.clone()))
        .await
        .expect("parse skill");

    assert_eq!(skill.metadata.name, "demo-skill");
    assert_eq!(skill.metadata.description, "Demo skill description");
    assert!(skill.content.contains("Content body"));
}

#[tokio::test]
async fn load_skills_from_directory() {
    let root = create_temp_skill_dir();
    let skill_dir = root.join("demo-skill");
    fs::create_dir_all(&skill_dir).expect("create skill dir");

    let content = r#"---
name: loader-skill
description: Loader skill description
---

# Loader Skill
Body.
"#;

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(&skill_path, content).expect("write skill file");

    let loader = SkillLoader::new();
    let skills = loader
        .load_from_directory(&root, &SkillSource::Custom(root.clone()))
        .await
        .expect("load skills");

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].metadata.name, "loader-skill");
}
