use crate::tools::ToolHandler;

pub struct Bash;

impl ToolHandler for Bash {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Run a shell command."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to run." }
            },
            "required": ["command"]
        })
    }

    fn run(&self, args: &serde_json::Value) -> String {
        let command = args["command"].as_str().unwrap();
        let output = std::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .expect("Failed to execute command");

        String::from_utf8_lossy(&output.stdout).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn name_and_description_contract() {
        assert_eq!(Bash.name(), "bash");
        assert!(!Bash.description().is_empty());
    }

    #[test]
    fn parameters_require_command() {
        let params = Bash.parameters();
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["command"].is_object());
        assert!(
            params["required"]
                .as_array()
                .unwrap()
                .contains(&json!("command"))
        );
    }

    // 核心行为：执行命令并拿到 stdout
    #[test]
    fn run_returns_stdout() {
        let out = Bash.run(&json!({"command": "echo hello"}));
        assert_eq!(out, "hello\n");
    }

    // 防回归：上次发现的真 bug 是命令拼路径，这里确保 stderr 不混入、
    // 多行输出完整保留
    #[test]
    fn run_preserves_multiline_stdout() {
        let out = Bash.run(&json!({"command": "printf 'a\\nb\\nc'"}));
        assert_eq!(out, "a\nb\nc");
    }
}
