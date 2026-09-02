use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    llm::{
        ContentPart, FinishReason::ToolCalls, Function, ImageUrl, Message, Tool, ToolCall, Usage,
        UserContent, completions,
    },
    tools::{Bash, Edit, Read, ToolHandler, Write},
};

pub struct Agent {
    client: Client,
    api_url: String,
    api_key: String,
    pub model: String,
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
                        name: Read.name().to_string(),
                        description: Read.description().to_string(),
                        parameters: Read.parameters(),
                    },
                },
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: Write.name().to_string(),
                        description: Write.description().to_string(),
                        parameters: Write.parameters(),
                    },
                },
                Tool {
                    tool_type: "function".to_string(),
                    function: Function {
                        name: Edit.name().to_string(),
                        description: Edit.description().to_string(),
                        parameters: Edit.parameters(),
                    },
                },
            ]),
        }
    }

    pub fn reset(&mut self, messages: Vec<Message>) {
        self.messages = messages;
    }

    pub async fn send(
        &mut self,
        content: &str,
        attachments: Option<&Vec<Attachment>>,
    ) -> Result<Vec<AgentMessage>, String> {
        self.messages.push(Message::User {
            content: if let Some(attachments) = attachments {
                let mut parts = vec![ContentPart::Text {
                    text: content.to_owned(),
                }];
                for attachment in attachments {
                    parts.push(ContentPart::ImageUrl {
                        image_url: ImageUrl {
                            url: attachment.get_url(),
                            detail: "auto".to_owned(),
                        },
                    });
                }
                UserContent::Parts(parts)
            } else {
                UserContent::Text(content.to_owned())
            },
            name: None,
        });
        self.agent_loop().await
    }

    async fn agent_loop(&mut self) -> Result<Vec<AgentMessage>, String> {
        let mut result = vec![];
        loop {
            let (response, elapsed) = match completions(
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
                    return Err(e);
                }
            };

            if let Some(choice) = response.choices.first() {
                self.messages.push(Message::Assistant {
                    content: choice.message.content.clone(),
                    name: None,
                    tool_calls: choice.message.tool_calls.clone(),
                });

                result.push(AgentMessage::Assistant {
                    content: choice.message.content.clone(),
                    tool_calls: choice.message.tool_calls.clone(),
                    usage: Some(response.usage),
                    elapsed,
                });

                // If the model is done, we're done.
                if choice.finish_reason != ToolCalls {
                    return Ok(result);
                }

                if let Some(tool_calls) = &choice.message.tool_calls {
                    for tool_call in tool_calls {
                        let args: Value =
                            serde_json::from_str(&tool_call.function.arguments).unwrap();
                        let res = match tool_call.function.name.as_str() {
                            "bash" => Bash.run(&args),
                            "read" => Read.run(&args),
                            "write" => Write.run(&args),
                            "edit" => Edit.run(&args),
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
        content: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
        usage: Option<Usage>,
        elapsed: Duration,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}

#[derive(Deserialize, Serialize)]
pub struct Attachment {
    pub content_type: String,
    pub data: String,
}

impl Attachment {
    pub fn get_url(&self) -> String {
        format!("data:{};base64,{}", self.content_type, self.data)
    }
}
