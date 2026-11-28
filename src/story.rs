// Story NPCs, loaded from story.toml. They get a name, a mind tier, trait
// overrides, authored methods and pre-written culture (marked AUTHORED in the
// logs, so it's never confused with what the world discovered).
//
// Example:
//
//   [[npc]]
//   name = "Old Builder"
//   mind = "story"            # simple | villager | story
//   x = 200
//   y = 300
//   lifespan = 1500
//   methods = ["bring_wood_home", "plant"]
//   traits = { sociability = 0.9, patience = 0.9 }
//
//   [[npc.knows]]
//   discovery = "WARMTH"
//   note = "Learned it from the mountain people"

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default)]
pub struct StoryFile {
    #[serde(default)]
    pub npc: Vec<StoryNpc>,
}

#[derive(Deserialize)]
pub struct StoryNpc {
    pub name: String,
    #[serde(default = "default_mind")]
    pub mind: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub lifespan: Option<f32>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub traits: HashMap<String, f32>,
    #[serde(default)]
    pub knows: Vec<Knows>,
    /// Extra pull toward exploring, for wanderers.
    #[serde(default)]
    pub wanderer: bool,
}

#[derive(Deserialize)]
pub struct Knows {
    pub discovery: String,
    #[serde(default)]
    pub note: String,
}

fn default_mind() -> String {
    "story".to_string()
}

pub fn load(path: &str) -> Result<StoryFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_example_format() {
        let s: StoryFile = toml::from_str(
            r#"
            [[npc]]
            name = "Old Builder"
            x = 200
            y = 300
            methods = ["bring_wood_home"]
            traits = { sociability = 0.9 }
            [[npc.knows]]
            discovery = "WARMTH"
            note = "from the mountains"
            "#,
        )
        .unwrap();
        assert_eq!(s.npc.len(), 1);
        assert_eq!(s.npc[0].mind, "story");
        assert_eq!(s.npc[0].knows[0].discovery, "WARMTH");
        assert_eq!(s.npc[0].traits["sociability"], 0.9);
    }
}
