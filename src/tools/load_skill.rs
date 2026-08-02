use crate::{skills::load_skill, tools::ToolHandler};

pub struct LoadSkill;

impl ToolHandler for LoadSkill {
    fn name(&self) -> &str {
        "load_skill"
    }
    fn description(&self) -> &str {
        "Load a skill from a file"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name, as shown in the available-skills list in the system prompt."
                }
            },
            "required": ["name"]
        })
    }

    fn run(&self, args: &serde_json::Value) -> String {
        let name = args.get("name").unwrap().as_str().unwrap();
        load_skill(&dirs::home_dir().unwrap().join(".nota-agent"), name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn name_and_description_contract() {
        assert_eq!(LoadSkill.name(), "load_skill");
        assert!(!LoadSkill.description().is_empty());
    }

    // name 必须是 snake_case，且与 main.rs system prompt 里提到的 `load_skill` 一致，
    // 否则模型按 prompt 调用时 name 对不上 → 走到 unknown tool 分支
    #[test]
    fn name_matches_system_prompt() {
        assert_eq!(LoadSkill.name(), "load_skill");
    }

    #[test]
    fn parameters_require_name() {
        let params = LoadSkill.parameters();
        assert_eq!(params["type"], "object");
        assert!(
            params["required"]
                .as_array()
                .unwrap()
                .contains(&json!("name"))
        );
    }
}
