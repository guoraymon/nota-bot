use crate::tools::ToolHandler;

pub struct Edit;

impl ToolHandler for Edit {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Replace text in file."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to edit." },
                "old_string": { "type": "string", "description": "The exact text to find." },
                "new_string": { "type": "string", "description": "The replacement text." },
                "replace_all": { "type": "boolean", "description": "Replace every occurrence. Defaults to false.", "default": false }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    fn run(&self, args: &serde_json::Value) -> String {
        let path = args["path"].as_str().unwrap();
        let old_string = args["old_string"].as_str().unwrap();
        let new_string = args["new_string"].as_str().unwrap();
        let replace_all = args["replace_all"].as_bool().unwrap_or(false);

        let content = std::fs::read_to_string(path).unwrap();
        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        std::fs::write(path, new_content).unwrap();
        "File edited successfully.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn name_and_description_contract() {
        assert_eq!(Edit.name(), "edit");
        assert!(!Edit.description().is_empty());
    }

    #[test]
    fn parameters_require_path_old_new() {
        let params = Edit.parameters();
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&json!("path")));
        assert!(required.contains(&json!("old_string")));
        assert!(required.contains(&json!("new_string")));
    }

    fn write_tmp(dir: &tempfile::TempDir, name: &str, content: &str) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path.to_str().unwrap().to_string()
    }

    #[test]
    fn run_replaces_first_occurrence() {
        let dir = tempdir().unwrap();
        let path = write_tmp(&dir, "a.txt", "foo bar foo");
        Edit.run(&json!({
            "path": path,
            "old_string": "foo",
            "new_string": "baz"
        }));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "baz bar foo");
    }

    #[test]
    fn run_replaces_all_when_flag_set() {
        let dir = tempdir().unwrap();
        let path = write_tmp(&dir, "a.txt", "foo bar foo");
        Edit.run(&json!({
            "path": path,
            "old_string": "foo",
            "new_string": "baz",
            "replace_all": true
        }));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "baz bar baz");
    }

    // 记录已知缺陷：old_string 不存在时静默不改，却报告成功
    // （问题 9，未修复前该测试守住现状；修复后应改成 assert content 不变 + 返回失败信息）
    #[test]
    fn run_silently_succeeds_when_no_match() {
        let dir = tempdir().unwrap();
        let original = "unchanged";
        let path = write_tmp(&dir, "a.txt", original);
        let msg = Edit.run(&json!({
            "path": path,
            "old_string": "NOT_PRESENT",
            "new_string": "x"
        }));
        // 内容未变
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        // 但却报告成功 —— 这是个 bug，测试显式记录该行为
        assert_eq!(msg, "File edited successfully.");
    }
}
