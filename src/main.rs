mod chat_completions;

use std::eprintln;

use crate::chat_completions::{Message, Request, Response};
use reqwest::Client;

const LLM_URL: &str = "https://api.deepseek.com/chat/completions";

#[tokio::main]
async fn main() {
    let api_key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set");
    let request = Request {
        model: "deepseek-v4-flash".to_string(),
        messages: vec![
            Message::System {
                content: "You are a helpful assistant.".to_string(),
                name: None,
            },
            Message::User {
                content: "Hello!".to_string(),
                name: None,
            },
        ],
    };
    println!("{}", serde_json::to_string_pretty(&request).unwrap());

    let client = Client::new();
    let response = client
        .post(LLM_URL)
        .bearer_auth(api_key)
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
    println!("{}", serde_json::to_string_pretty(&response).unwrap());
}
