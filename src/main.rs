mod agent;
mod bot;
mod db;
mod llm;
mod skills;
mod tools;

use std::{collections::HashMap, sync::Arc};

use crate::{
    agent::{Agent, AgentMessage},
    bot::{Bot, BotApi, IncomingMessage, TokenManager},
    db::DbMessage,
    llm::Message,
};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};
use tokio::sync::mpsc;

#[derive(Deserialize)]
struct Config {
    app_id: String,
    client_secret: String,
    #[serde(rename = "defaultProvider")]
    default_provider: String,
    #[serde(rename = "defaultModel")]
    default_model: String,
    providers: HashMap<String, ProviderConfig>,
}

#[derive(Deserialize)]
struct ProviderConfig {
    url: String,
    key: String,
    models: HashMap<String, ModelConfig>,
}

#[derive(Deserialize)]
struct ModelConfig {
    model: String,
}

#[tokio::main]
async fn main() {
    let nota_agent_home = dirs::home_dir().unwrap().join(".nota-bot");

    let db_path = nota_agent_home.join("default.db");
    let db_opts = SqliteConnectOptions::new()
        .create_if_missing(true)
        .filename(&db_path);
    let mut conn = SqliteConnection::connect_with(&db_opts).await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS conversations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(&mut conn)
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id INTEGER NOT NULL REFERENCES conversations(id),
            role TEXT NOT NULL,
            content TEXT,
            tool_calls TEXT,
            tool_call_id TEXT,
            created_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(&mut conn)
    .await
    .unwrap();

    let conversation = match sqlx::query("SELECT * FROM conversations ORDER BY id DESC LIMIT 1")
        .fetch_optional(&mut conn)
        .await
        .unwrap()
    {
        Some(row) => row,
        None => {
            sqlx::query("INSERT INTO conversations (model, created_at) VALUES (?, ?)")
                .bind("deepseek-v4-flash")
                .bind(Utc::now().timestamp())
                .execute(&mut conn)
                .await
                .unwrap();
            let row = sqlx::query("SELECT * FROM conversations ORDER BY id DESC LIMIT 1")
                .fetch_one(&mut conn)
                .await
                .unwrap();

            let conv_id: i64 = row.get("id");
            sqlx::query("INSERT INTO messages (conversation_id, role, content, created_at) VALUES (?, ?, ?, ?)")
            .bind(conv_id)
            .bind("system")
            .bind(format!(
                    "You are a helpful assistant.\nSkills available:\n{}\nUse load_skill to get full details when needed.",
                    skills::list_skills(&dirs::home_dir().unwrap().join(".nota-bot")),
                ))
                .bind(Utc::now().timestamp())
                .execute(&mut conn)
                .await.unwrap();

            row
        }
    };

    let conv_id: i64 = conversation.get("id");

    let content = std::fs::read_to_string(nota_agent_home.join("config.json")).unwrap();
    let config: Config = serde_json::from_str(&content).unwrap();
    let default_provider = config
        .providers
        .get(&config.default_provider)
        .expect("default provider error");
    let default_model = default_provider
        .models
        .get(&config.default_model)
        .expect("default model error");

    let client = Client::new();
    let token_manager = Arc::new(TokenManager::new(
        &client,
        &config.app_id,
        &config.client_secret,
    ));
    let mut bot = Bot::new(&client, token_manager.clone());
    let bot_api = BotApi::new(&client, token_manager.clone());

    let mut conversation_store = ConversationStore { conn, conv_id };
    let mut agent: Agent = Agent::new(
        default_provider.url.clone(),
        default_provider.key.clone(),
        default_model.model.clone(),
        conversation_store.get().await,
    );

    let (tx, mut rx) = mpsc::channel::<IncomingMessage>(100);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            conversation_store
                .append("user", Some(msg.content.to_string()), None, None)
                .await;
            let mut last_content = None;
            let result = agent.send(&msg.content).await;

            let mut prompt_tokens = 0;
            let mut completion_tokens = 0;
            let mut cached_tokens = 0;

            for message in result {
                match message {
                    AgentMessage::Assistant {
                        content,
                        tool_calls,
                        usage,
                    } => {
                        let tool_calls_json = tool_calls
                            .as_ref()
                            .map(|tc| serde_json::to_string(&tc).unwrap());

                        conversation_store
                            .append("assistant", Some(content.clone()), tool_calls_json, None)
                            .await;

                        last_content = Some(content);

                        if let Some(usage) = usage {
                            prompt_tokens += usage.prompt_tokens;
                            completion_tokens += usage.completion_tokens;
                            if let Some(hit_tokens) = usage.prompt_cache_hit_tokens {
                                cached_tokens += hit_tokens;
                            } else if let Some(prompt_tokens_details) = usage.prompt_tokens_details
                            {
                                cached_tokens += prompt_tokens_details.cached_tokens;
                            }
                        }
                    }
                    AgentMessage::Tool {
                        content,
                        tool_call_id,
                    } => {
                        conversation_store
                            .append(
                                "tool",
                                Some(content.clone()),
                                None,
                                Some(tool_call_id.clone()),
                            )
                            .await;
                    }
                }
            }

            if let Some(last_content) = last_content {
                let content = format!(
                    "{last_content}\n\n↑{prompt_tokens} ↓{completion_tokens} CH{:.2}%",
                    if cached_tokens > 0 {
                        cached_tokens as f64 / prompt_tokens as f64 * 100f64
                    } else {
                        0f64
                    }
                );
                bot_api
                    .send_user_msg(&msg.user_openid, &msg.msg_id, &content)
                    .await;
            }
        }
    });

    bot.run(tx).await;

    return;
}

struct ConversationStore {
    conn: SqliteConnection,
    conv_id: i64,
}

impl ConversationStore {
    pub async fn get(&mut self) -> Vec<Message> {
        let db_messages =
            sqlx::query_as::<_, DbMessage>("SELECT * FROM messages WHERE conversation_id = ?")
                .bind(self.conv_id)
                .fetch_all(&mut self.conn)
                .await
                .unwrap();
        let messages = db_messages
            .into_iter()
            .map(|db_message| match db_message.role.as_str() {
                "system" => Message::System {
                    content: db_message.content.unwrap_or_default(),
                    name: None,
                },
                "user" => Message::User {
                    content: db_message.content.unwrap_or_default(),
                    name: None,
                },
                "assistant" => Message::Assistant {
                    content: db_message.content,
                    name: None,
                    tool_calls: db_message
                        .tool_calls
                        .map(|s| serde_json::from_str(&s).unwrap()),
                },
                "tool" => Message::Tool {
                    content: db_message.content.unwrap_or_default(),
                    tool_call_id: db_message.tool_call_id.unwrap_or_default(),
                },
                _ => {
                    panic!("Unknown role: {}", db_message.role)
                }
            })
            .collect::<Vec<Message>>();
        messages
    }

    pub async fn append(
        &mut self,
        role: &str,
        content: Option<String>,
        tool_calls: Option<String>,
        tool_call_id: Option<String>,
    ) {
        sqlx::query(
            "INSERT INTO messages (conversation_id, role, content, tool_calls, tool_call_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(self.conv_id)
        .bind(role)
        .bind(content)
        .bind(tool_calls)
        .bind(tool_call_id)
        .bind(Utc::now().timestamp())
        .execute(&mut self.conn)
        .await
        .unwrap();
    }
}
