use crate::tools::ToolHandler;

pub struct Read;

impl ToolHandler for Read {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read file contents."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read."
                }
            },
            "required": ["path"]
        })
    }

    fn run(&self, args: &serde_json::Value) -> String {
        let path = args["path"].as_str().unwrap();
        std::fs::read_to_string(path).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::NamedTempFile;

    #[test]
    fn name_and_description_contract() {
        assert_eq!(Read.name(), "read");
        assert!(!Read.description().is_empty());
    }

    #[test]
    fn parameters_require_path() {
        let params = Read.parameters();
        assert_eq!(params["type"], "object");
        assert!(
            params["required"]
                .as_array()
                .unwrap()
                .contains(&json!("path"))
        );
    }

    #[test]
    fn run_reads_file_contents() {
        let tmp = NamedTempFile::new().unwrap();
        std::fs::write(&tmp, "hello world").unwrap();
        let path = tmp.path().to_str().unwrap();
        let out = Read.run(&json!({"path": path}));
        assert_eq!(out, "hello world");
    }

    // 文件不存在时返回空串（当前行为；记录现状，便于以后改进时察觉）
    #[test]
    fn run_returns_empty_when_missing() {
        let out = Read.run(&json!({"path": "/this/does/not/exist/xyz"}));
        assert_eq!(out, "");
    }
}
