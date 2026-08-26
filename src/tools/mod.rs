use serde_json::Value;

pub mod bash;
pub mod edit;
pub mod read;
pub mod write;

pub use bash::Bash;
pub use edit::Edit;
pub use read::Read;
pub use write::Write;

pub trait ToolHandler {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn run(&self, args: &Value) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{bash::Bash, edit::Edit, read::Read, write::Write};

    // 所有 handler 的 parameters() 都应是 object 类型（合法 JSON Schema 顶层）
    #[test]
    fn parameters_are_objects() {
        for h in [&Bash as &dyn ToolHandler, &Read, &Write, &Edit] {
            let params = h.parameters();
            assert!(
                params.is_object(),
                "{} parameters should be a JSON object, got: {params}",
                h.name()
            );
        }
    }

    // 所有 handler 的 name 都非空、唯一、且符合 snake_case
    // （API 要求 function name 匹配 ^[a-zA-Z0-9_-]+$；空格会导致模型无法调用 / API 拒绝）
    #[test]
    fn names_are_non_empty_unique_and_snake_case() {
        let names: Vec<&str> = [&Bash as &dyn ToolHandler, &Read, &Write, &Edit]
            .into_iter()
            .map(|h| h.name())
            .collect();
        assert!(names.iter().all(|n| !n.is_empty()), "empty name: {names:?}");
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "duplicate names: {names:?}");
        for n in &names {
            assert!(
                n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "name contains invalid chars (space/unicode etc.): {n}"
            );
        }
    }
}
