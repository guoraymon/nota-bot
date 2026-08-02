mod chat_completions;

use crate::chat_completions::{
    FinishReason::ToolCalls, Function, Message, Request, Response, Tool,
};
use reqwest::Client;
use serde_json::{Value, json};

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
        tools: Some(vec![Tool {
            tool_type: "function".to_string(),
            function: Function {
                name: "bash".to_string(),
                description: "Run a bash command".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The shell command to run." }
                    },
                    "required": ["command"]
                }),
            },
        }]),
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

        if let Some(choice) = response.choices.get(0) {
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
                    if tool_call.function.name == "bash" {
                        let args: Value =
                            serde_json::from_str(&tool_call.function.arguments).unwrap();
                        let command = args.get("command").unwrap().as_str().unwrap();
                        request.messages.push(Message::Tool {
                            content: run_bash(command),
                            tool_call_id: tool_call.id.clone(),
                        });
                    }
                });
            };
        }
    }
}

fn run_bash(command: &str) -> String {
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}
