use crate::tools::ToolHandler;

pub struct Write;

impl ToolHandler for Write {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to file."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to write." },
                "content": { "type": "string", "description": "The text content to write." }
            },
            "required": ["path", "content"]
        })
    }

    fn run(&self, args: &serde_json::Value) -> String {
        let path = args.get("path").unwrap().as_str().unwrap();
        let content = args.get("content").unwrap().as_str().unwrap();

        std::fs::write(path, content).expect("Failed to write file.");

        "File written successfully.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn name_and_description_contract() {
        assert_eq!(Write.name(), "write");
        assert!(!Write.description().is_empty());
    }

    #[test]
    fn parameters_require_path_and_content() {
        let params = Write.parameters();
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&json!("path")));
        assert!(required.contains(&json!("content")));
    }

    #[test]
    fn run_creates_file_with_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let path_str = path.to_str().unwrap();
        let msg = Write.run(&json!({"path": path_str, "content": "new content"}));
        assert_eq!(msg, "File written successfully.");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "new content");
    }

    // 覆盖已存在的文件（而非追加）
    #[test]
    fn run_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "old").unwrap();
        Write.run(&json!({"path": path.to_str().unwrap(), "content": "new"}));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "new");
    }
}
