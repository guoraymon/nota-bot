use serde_json::Value;

pub mod bash;
pub mod edit_file;
pub mod glob;
pub mod read_file;
pub mod write_file;

pub trait ToolHandler {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn run(&self, args: &Value) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{
        bash::Bash, edit_file::EditFile, glob::Glob, read_file::ReadFile, write_file::WriteFile,
    };

    // 所有 handler 的 parameters() 都应是 object 类型（合法 JSON Schema 顶层）
    #[test]
    fn parameters_are_objects() {
        for h in [
            &Bash as &dyn ToolHandler,
            &ReadFile,
            &WriteFile,
            &EditFile,
            &Glob,
        ] {
            let params = h.parameters();
            assert!(
                params.is_object(),
                "{} parameters should be a JSON object, got: {params}",
                h.name()
            );
        }
    }

    // 所有 handler 的 name 都非空且唯一（agent_loop 靠 name 分发）
    #[test]
    fn names_are_non_empty_and_unique() {
        let names: Vec<&str> = [
            &Bash as &dyn ToolHandler,
            &ReadFile,
            &WriteFile,
            &EditFile,
            &Glob,
        ]
        .into_iter()
        .map(|h| h.name())
        .collect();
        assert!(names.iter().all(|n| !n.is_empty()), "empty name: {names:?}");
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "duplicate names: {names:?}");
    }
}
