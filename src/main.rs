mod agent;
mod bot;
mod chat_completions;
mod db;
mod skills;
mod tools;

use crate::{
    agent::{Agent, AgentObserver},
    bot::Bot,
    chat_completions::Message,
    db::DbMessage,
};
use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};

const LLM_URL: &str = "https://api.deepseek.com/chat/completions";

#[tokio::main]
async fn main() {
    let nota_agent_home = dirs::home_dir().unwrap().join(".nota-bot");
    let content = std::fs::read_to_string(nota_agent_home.join("config.json")).unwrap();
    let cfg: Value = serde_json::from_str(&content).unwrap();
    let app_id = cfg["app_id"].as_str().expect("app_id is null");
    let client_secret = cfg["client_secret"]
        .as_str()
        .expect("client_secret is null");

    let client = Client::new();
    let mut bot = Bot::new();
    bot.run(&client, app_id, client_secret).await;
    return;

    let user_msg = std::env::args().nth(1).expect("send a message");
    let api_key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set");

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
    let db_messages =
        sqlx::query_as::<_, DbMessage>("SELECT * FROM messages WHERE conversation_id = ?")
            .bind(conv_id)
            .fetch_all(&mut conn)
            .await
            .unwrap();
    let mut messages = db_messages
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
    messages.push(Message::User {
        content: user_msg.clone(),
        name: None,
    });
    sqlx::query(
        "INSERT INTO messages (conversation_id, role, content, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(conv_id)
    .bind("user")
    .bind(user_msg.clone())
    .bind(Utc::now().timestamp())
    .execute(&mut conn)
    .await
    .unwrap();

    let mut agent = Agent::new(LLM_URL.to_string(), api_key, conversation.get("model"))
        .with_observer(ConversationObserver::new(conv_id, conn));
    agent.send(messages).await;
}

struct ConversationObserver {
    conv_id: i64,
    conn: SqliteConnection,
}

impl ConversationObserver {
    pub fn new(conv_id: i64, conn: SqliteConnection) -> Self {
        Self { conv_id, conn }
    }
}

impl AgentObserver for ConversationObserver {
    async fn message_update(&mut self, message: &Message) {
        match message {
            Message::Assistant {
                content,
                name,
                tool_calls,
            } => {
                let tool_calls_json = tool_calls
                    .as_ref()
                    .map(|tc| serde_json::to_string(&tc).unwrap());
                sqlx::query(
        "INSERT INTO messages (conversation_id, role, content, tool_calls, created_at) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(self.conv_id)
                .bind("assistant")
                .bind(content)
                .bind(tool_calls_json)
                .bind(Utc::now().timestamp())
                .execute(&mut self.conn)
                .await
                   .unwrap();
            }
            Message::Tool {
                content,
                tool_call_id,
            } => {
                sqlx::query(
                    "INSERT INTO messages (conversation_id, role, content, tool_call_id, created_at) VALUES (?, ?, ?, ?, ?)",
                            )
                            .bind(self.conv_id)
                            .bind("tool")
                            .bind(content)
                            .bind(tool_call_id)
                            .bind(Utc::now().timestamp())
                            .execute(&mut self.conn)
                            .await
                            .unwrap();
            }
            Message::System { content, name } => todo!(),
            Message::User { content, name } => todo!(),
        }
    }
}
