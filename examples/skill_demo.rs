use agent_lib::skills::{SkillLoader, SkillRegistry};

#[tokio::main]
async fn main() -> agent_lib::AgentResult<()> {
    let loader = SkillLoader::new();
    let skills = loader.load_all().await?;

    let mut registry = SkillRegistry::new();
    for skill in skills {
        registry.register(skill);
    }

    let entries = registry.list();
    println!("Found {} skill(s):", entries.len());
    for entry in entries {
        println!(
            "- {} ({}) [{}]",
            entry.name, entry.description, entry.source
        );
    }

    Ok(())
}
