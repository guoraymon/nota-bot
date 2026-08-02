use crate::tools::ToolHandler;

pub struct Glob;

impl ToolHandler for Glob {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files by pattern."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern, e.g. '**/*.rs' or '*.txt'." },
                "path": { "type": "string", "description": "Optional directory to search in. Defaults to the current directory.", "default": "." }
            },
            "required": ["pattern"]
        })
    }

    fn run(&self, args: &serde_json::Value) -> String {
        let pattern = args["pattern"].as_str().unwrap();
        let path = args["path"].as_str().unwrap_or(".");
        glob::glob(&format!("{}/{}", path, pattern))
            .unwrap()
            .map(|p| p.unwrap().display().to_string())
            .collect::<Vec<String>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn name_and_description_contract() {
        assert_eq!(Glob.name(), "glob");
        assert!(!Glob.description().is_empty());
    }

    #[test]
    fn parameters_require_pattern() {
        let params = Glob.parameters();
        assert!(
            params["required"]
                .as_array()
                .unwrap()
                .contains(&json!("pattern"))
        );
    }

    // 防回归：问题 1 —— path 与 pattern 之间必须有 '/'
    // 之前是 format!("{}{}", path, pattern)，拼出 ".**/*.rs" 导致匹配错误
    #[test]
    fn run_finds_files_with_explicit_path() {
        let dir = tempdir().unwrap();
        // 建一个 .rs 文件
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();

        let out = Glob.run(&json!({
            "path": dir.path().to_str().unwrap(),
            "pattern": "*.rs"
        }));
        assert!(out.contains("a.rs"), "got: {out}");
        assert!(!out.contains("b.txt"), "got: {out}");
    }

    // path 省略时默认当前目录；至少能跑通不 panic
    #[test]
    fn run_defaults_path_to_dot() {
        // 用一个稳定的相对 pattern，避免依赖具体文件存在性
        let out = Glob.run(&json!({"pattern": "Cargo.toml"}));
        assert!(out.contains("Cargo.toml"), "got: {out}");
    }
}
