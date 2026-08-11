use reqwest::Client;
use serde_json::Value;

use crate::{
    chat_completions::{FinishReason::ToolCalls, Function, Message, Request, Response, Tool},
    tools::{Bash, EditFile, Glob, LoadSkill, ReadFile, ToolHandler, WriteFile},
};

pub trait AgentObserver {
    async fn message_update(&mut self, _message: &Message) {}
}

pub struct Agent<O: AgentObserver> {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
    observer: Option<O>,
}

impl<O: AgentObserver> Agent<O> {
    pub fn new(api_url: String, api_key: String, model: String) -> Self {
        let client = Client::new();

        Self {
            client,
            api_url,
            api_key,
            model,
            observer: None,
        }
    }

    pub fn with_observer(mut self, observer: O) -> Self {
        self.observer = Some(observer);
        self
    }

    pub async fn send(&mut self, messages: Vec<Message>) -> Vec<Message> {
        let mut request = Request {
            model: self.model.clone(),
            messages: messages,
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
        };

        self.agent_loop(&mut request).await
    }

    async fn agent_loop(&mut self, request: &mut Request) -> Vec<Message> {
        let mut result = vec![];
        loop {
            println!(
                "request: {}",
                serde_json::to_string_pretty(&request).unwrap()
            );
            let response = self
                .client
                .post(self.api_url.clone())
                .bearer_auth(self.api_key.clone())
                .json(&request)
                .send()
                .await
                .expect("request failed");
            if !response.status().is_success() {
                eprintln!(
                    "HTTP {}: {}",
                    response.status(),
                    response.text().await.unwrap()
                );
                return result;
            }
            let response: Response = response.json().await.unwrap();
            println!(
                "response: {}",
                serde_json::to_string_pretty(&response).unwrap()
            );

            if let Some(choice) = response.choices.first() {
                let message = Message::Assistant {
                    content: choice.message.content.clone(),
                    name: None,
                    tool_calls: choice.message.tool_calls.clone(),
                };
                if let Some(obs) = self.observer.as_mut() {
                    obs.message_update(&message).await;
                }
                result.push(message.clone());
                request.messages.push(message);

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
                        if let Some(obs) = self.observer.as_mut() {
                            obs.message_update(&message).await;
                        }
                        result.push(message.clone());
                        request.messages.push(message);
                    }
                };
            }
        }
    }
}
