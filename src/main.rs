mod chat_completions;
mod tools;

use crate::{
    chat_completions::{FinishReason::ToolCalls, Function, Message, Request, Response, Tool},
    tools::{
        ToolHandler, bash::Bash, edit_file::EditFile, glob::Glob, read_file::ReadFile,
        write_file::WriteFile,
    },
};
use reqwest::Client;
use serde_json::Value;

const LLM_URL: &str = "https://api.deepseek.com/chat/completions";

#[tokio::main]
async fn main() {
    let user_msg = std::env::args().nth(1).expect("send a message");
    let api_key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set");
    let request = Request {
        model: "deepseek-v4-flash".to_string(),
        messages: vec![
            Message::System {
                content: "You are a helpful assistant.".to_string(),
                name: None,
            },
            Message::User {
                content: user_msg.to_string(),
                name: None,
            },
        ],
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
        ]),
    };

    let client = Client::new();
    agent_loop(client, api_key, request).await;
}

async fn agent_loop(client: Client, api_key: String, mut request: Request) {
    loop {
        println!(
            "request: {}",
            serde_json::to_string_pretty(&request).unwrap()
        );
        let response = client
            .post(LLM_URL)
            .bearer_auth(api_key.clone())
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
            return;
        }
        let response: Response = response.json().await.unwrap();
        println!(
            "response: {}",
            serde_json::to_string_pretty(&response).unwrap()
        );

        if let Some(choice) = response.choices.first() {
            request.messages.push(Message::Assistant {
                content: choice.message.content.clone(),
                name: None,
                tool_calls: choice.message.tool_calls.clone(),
            });

            // If the model is done, we're done.
            if choice.finish_reason != ToolCalls {
                return;
            }

            if let Some(tool_calls) = &choice.message.tool_calls {
                tool_calls.iter().for_each(|tool_call| {
                    let args: Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
                    let result = match tool_call.function.name.as_str() {
                        "bash" => Bash.run(&args),
                        "read_file" => ReadFile.run(&args),
                        "write_file" => WriteFile.run(&args),
                        "edit_file" => EditFile.run(&args),
                        "glob" => Glob.run(&args),
                        other => format!("unknown tool: {other}"),
                    };
                    request.messages.push(Message::Tool {
                        content: result,
                        tool_call_id: tool_call.id.clone(),
                    });
                });
            };
        }
    }
}
