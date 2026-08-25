use reqwest::Client;
use serde_json::Value;

use crate::{
    llm::{FinishReason::ToolCalls, Function, Message, Tool, ToolCall, Usage, completions},
    tools::{Bash, EditFile, Glob, LoadSkill, ReadFile, ToolHandler, WriteFile},
};

pub struct Agent {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
    messages: Vec<Message>,
    tools: Option<Vec<Tool>>,
}

impl Agent {
    pub fn new(api_url: String, api_key: String, model: String, messages: Vec<Message>) -> Self {
        let client = Client::new();

        Self {
            client,
            api_url,
            api_key,
            model,
            messages,
            tools: Some(vec![
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: Bash.name().to_string(),
                        description: Bash.description().to_string(),
                        parameters: Bash.parameters(),
                    },
                },
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: ReadFile.name().to_string(),
                        description: ReadFile.description().to_string(),
                        parameters: ReadFile.parameters(),
                    },
                },
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: WriteFile.name().to_string(),
                        description: WriteFile.description().to_string(),
                        parameters: WriteFile.parameters(),
                    },
                },
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: EditFile.name().to_string(),
                        description: EditFile.description().to_string(),
                        parameters: EditFile.parameters(),
                    },
                },
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: Glob.name().to_string(),
                        description: Glob.description().to_string(),
                        parameters: Glob.parameters(),
                    },
                },
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: LoadSkill.name().to_string(),
                        description: LoadSkill.description().to_string(),
                        parameters: LoadSkill.parameters(),
                    },
                },
            ]),
        }
    }

    pub async fn send(&mut self, content: &str) -> Vec<AgentMessage> {
        self.messages.push(Message::User {
            content: content.to_owned(),
            name: None,
        });
        self.agent_loop().await
    }

    async fn agent_loop(&mut self) -> Vec<AgentMessage> {
        let mut result = vec![];
        loop {
            let response = match completions(
                &self.client,
                &self.api_url,
                &self.api_key,
                &self.model,
                self.messages.clone(),
                self.tools.clone(),
            )
            .await
            {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("completions failed: {e}");
                    return result;
                }
            };

            if let Some(choice) = response.choices.first() {
                self.messages.push(Message::Assistant {
                    content: choice.message.content.clone(),
                    name: None,
                    tool_calls: choice.message.tool_calls.clone(),
                });

                result.push(AgentMessage::Assistant {
                    content: choice.message.content.clone().unwrap_or_default(),
                    tool_calls: choice.message.tool_calls.clone(),
                    usage: Some(response.usage),
                });

                // If the model is done, we're done.
                if choice.finish_reason != ToolCalls {
                    return result;
                }

                if let Some(tool_calls) = &choice.message.tool_calls {
                    for tool_call in tool_calls {
                        let args: Value =
                            serde_json::from_str(&tool_call.function.arguments).unwrap();
                        let res = match tool_call.function.name.as_str() {
                            "bash" => Bash.run(&args),
                            "read_file" => ReadFile.run(&args),
                            "write_file" => WriteFile.run(&args),
                            "edit_file" => EditFile.run(&args),
                            "glob" => Glob.run(&args),
                            "load_skill" => LoadSkill.run(&args),
                            other => format!("unknown tool: {other}"),
                        };
                        let message = Message::Tool {
                            content: res.clone(),
                            tool_call_id: tool_call.id.clone(),
                        };
                        self.messages.push(message);

                        result.push(AgentMessage::Tool {
                            content: res.clone(),
                            tool_call_id: tool_call.id.clone(),
                        });
                    }
                };
            }
        }
    }
}

pub enum AgentMessage {
    Assistant {
        content: String,
        tool_calls: Option<Vec<ToolCall>>,
        usage: Option<Usage>,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}
