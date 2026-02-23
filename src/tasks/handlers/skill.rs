//! Skill相关操作handlers
//!
//! 处理技能列表、获取、应用和读取技能文件等操作。

use std::path::PathBuf;
use tracing::debug;

use crate::error::AgentResult;
use crate::protocol::Event;
use crate::session::Session;
use crate::skills::{Skill, SkillLoader, SkillSource};

/// 处理列出技能请求
pub async fn handle_list_skills(
    sess: &Session,
    cwds: Vec<PathBuf>,
    force_reload: bool,
) {
    debug!(cwds = ?cwds, force_reload = force_reload, "Handling list skills");

    let skills = match load_skills_for_request(sess, &cwds).await {
        Ok(list) => list,
        Err(err) => {
            sess.emit_event(Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            Vec::new()
        }
    };

    let mut registry = crate::skills::SkillRegistry::new();
    for skill in skills {
        registry.register(skill);
    }

    let entries = registry.list();

    sess.emit_event(Event::ListSkillsResponse { skills: entries })
        .await;

    if force_reload {
        sess.emit_event(Event::Warning {
            message: "Skills list refreshed".to_string(),
        })
        .await;
    }
}

/// 处理获取技能内容请求
pub async fn handle_get_skill(sess: &Session, name: String) {
    debug!(name = %name, "Handling get skill");

    let skill = match load_skill_by_name(sess, &name).await {
        Ok(Some(skill)) => skill,
        Ok(None) => {
            sess.emit_event(Event::Warning {
                message: format!("未找到技能: {name}"),
            })
            .await;
            return;
        }
        Err(err) => {
            sess.emit_event(Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            return;
        }
    };

    let auxiliary_files = skill
        .auxiliary_files
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    sess.emit_event(Event::SkillContent {
        name: skill.metadata.name.clone(),
        content: skill.content.clone(),
        auxiliary_files,
    })
    .await;
}

/// 处理应用技能请求
pub async fn handle_apply_skill(sess: &Session, name: String) {
    debug!(name = %name, "Handling apply skill");

    let skill = match load_skill_by_name(sess, &name).await {
        Ok(Some(skill)) => skill,
        Ok(None) => {
            sess.emit_event(Event::Warning {
                message: format!("未找到技能: {name}"),
            })
            .await;
            return;
        }
        Err(err) => {
            sess.emit_event(Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            return;
        }
    };

    sess.emit_event(Event::SkillApplied {
        name: skill.metadata.name.clone(),
    })
    .await;
}

/// 处理读取技能文件请求
pub async fn handle_read_skill_file(sess: &Session, skill_name: String, file_path: String) {
    debug!(
        skill_name = %skill_name,
        file_path = %file_path,
        "Handling read skill file"
    );

    let skill = match load_skill_by_name(sess, &skill_name).await {
        Ok(Some(skill)) => skill,
        Ok(None) => {
            sess.emit_event(Event::Warning {
                message: format!("未找到技能: {skill_name}"),
            })
            .await;
            return;
        }
        Err(err) => {
            sess.emit_event(Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            return;
        }
    };

    let requested = skill.directory.join(&file_path);
    let skill_dir = match tokio::fs::canonicalize(&skill.directory).await {
        Ok(dir) => dir,
        Err(err) => {
            sess.emit_event(Event::Warning {
                message: format!("解析技能目录失败: {err}"),
            })
            .await;
            return;
        }
    };

    let requested = match tokio::fs::canonicalize(&requested).await {
        Ok(path) => path,
        Err(err) => {
            sess.emit_event(Event::Warning {
                message: format!("读取技能文件失败: {err}"),
            })
            .await;
            return;
        }
    };

    if !requested.starts_with(&skill_dir) {
        sess.emit_event(Event::Warning {
            message: "技能文件路径无效".to_string(),
        })
        .await;
        return;
    }

    let content = match tokio::fs::read_to_string(&requested).await {
        Ok(content) => content,
        Err(err) => {
            sess.emit_event(Event::Warning {
                message: format!("读取技能文件失败: {err}"),
            })
            .await;
            return;
        }
    };

    sess.emit_event(Event::SkillFileContent {
        skill_name,
        file_path,
        content,
    })
    .await;
}

/// 按名称加载技能
async fn load_skill_by_name(sess: &Session, name: &str) -> AgentResult<Option<Skill>> {
    if let Some(registry) = sess.get_skill_registry() {
        if let Some(skill) = registry.get(name) {
            return Ok(Some(skill.clone()));
        }
    }

    let skills = load_skills_for_request(sess, &Vec::new()).await?;
    Ok(skills.into_iter().find(|skill| skill.metadata.name == name))
}

/// 加载请求所需的技能列表
async fn load_skills_for_request(
    sess: &Session,
    cwds: &[PathBuf],
) -> AgentResult<Vec<Skill>> {
    let loader = SkillLoader::new();
    let mut skills = Vec::new();

    if let Some(config) = sess.get_skill_config() {
        if !config.enabled {
            return Ok(skills);
        }

        if !cwds.is_empty() {
            for cwd in cwds {
                let dir = cwd.join(".cursor").join("skills");
                let mut loaded = loader
                    .load_from_directory(&dir, &SkillSource::Custom(dir.clone()))
                    .await?;
                skills.append(&mut loaded);
            }
            return Ok(skills);
        }

        if let Some(personal_dir) = &config.personal_dir {
            let mut loaded = loader
                .load_from_directory(personal_dir, &SkillSource::Personal)
                .await?;
            skills.append(&mut loaded);
        } else if let Some(home) = skill_home_dir() {
            let dir = home.join(".cursor").join("skills");
            let mut loaded = loader
                .load_from_directory(&dir, &SkillSource::Personal)
                .await?;
            skills.append(&mut loaded);
        }

        if config.project_dirs.is_empty() {
            let dir = PathBuf::from(".cursor").join("skills");
            let mut loaded = loader
                .load_from_directory(&dir, &SkillSource::Project)
                .await?;
            skills.append(&mut loaded);
        } else {
            for dir in &config.project_dirs {
                let mut loaded = loader
                    .load_from_directory(dir, &SkillSource::Project)
                    .await?;
                skills.append(&mut loaded);
            }
        }

        return Ok(skills);
    }

    if !cwds.is_empty() {
        for cwd in cwds {
            let dir = cwd.join(".cursor").join("skills");
            let mut loaded = loader
                .load_from_directory(&dir, &SkillSource::Custom(dir.clone()))
                .await?;
            skills.append(&mut loaded);
        }
        return Ok(skills);
    }

    skills = loader.load_all().await?;
    Ok(skills)
}

/// 获取技能主目录
fn skill_home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(home));
    }
    std::env::var("HOME").ok().map(PathBuf::from)
}
