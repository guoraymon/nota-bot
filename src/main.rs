mod agent;
mod bot;
mod chat_completions;
mod db;
mod skills;
mod tools;

use crate::{
    agent::{Agent, AgentObserver},
    bot::{Bot, BotApi, TokenManager},
    chat_completions::Message,
    db::DbMessage,
};
use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};
use tokio::sync::mpsc;

const LLM_URL: &str = "https://api.deepseek.com/chat/completions";

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
    let cfg: Value = serde_json::from_str(&content).unwrap();
    let app_id = cfg["app_id"].as_str().expect("app_id is null");
    let client_secret = cfg["client_secret"]
        .as_str()
        .expect("client_secret is null");

    let mut conversation_store = ConversationStore { conn, conv_id };
    let client = Client::new();
    let tokens = TokenManager::new(&client, app_id, client_secret);
    let mut bot = Bot::new(&client, app_id, client_secret);
    let mut bot_api = BotApi::new(&client, tokens);

    let api_key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set");
    let mut agent: Agent<ConversationObserver> =
        Agent::new(LLM_URL.to_string(), api_key, conversation.get("model"));

    let (tx, mut rx) = mpsc::channel::<IncomingMessage>(100);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            conversation_store
                .append("user", Some(msg.content.to_string()), None, None)
                .await;
            let messages = agent.send(conversation_store.get().await).await;
            for message in &messages {
                match message {
                    Message::System {
                        content: _,
                        name: _,
                    } => {}
                    Message::User {
                        content: _,
                        name: _,
                    } => {}
                    Message::Assistant {
                        content,
                        name: _,
                        tool_calls,
                    } => {
                        let tool_calls_json = tool_calls
                            .as_ref()
                            .map(|tc| serde_json::to_string(&tc).unwrap());

                        conversation_store
                            .append("assistant", content.clone(), tool_calls_json, None)
                            .await;
                    }
                    Message::Tool {
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

            if let Some(Message::Assistant {
                content: Some(c),
                name: _,
                tool_calls: _,
            }) = messages.last()
            {
                bot_api
                    .send_user_msg(&msg.user_openid, &msg.msg_id, c)
                    .await;
            }
        }
    });

    bot.run(tx).await;

    return;
}

pub struct IncomingMessage {
    user_openid: String,
    msg_id: String,
    content: String,
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

pub struct ConversationObserver {
    conversation_store: ConversationStore,
}

impl ConversationObserver {
    fn new(conversation_store: ConversationStore) -> Self {
        Self { conversation_store }
    }
}

impl AgentObserver for ConversationObserver {
    async fn message_update(&mut self, message: &Message) {
        match message {
            Message::System {
                content: _,
                name: _,
            } => {}
            Message::User {
                content: _,
                name: _,
            } => {}
            Message::Assistant {
                content,
                name: _,
                tool_calls,
            } => {
                let tool_calls_json = tool_calls
                    .as_ref()
                    .map(|tc| serde_json::to_string(&tc).unwrap());

                self.conversation_store
                    .append("assistant", content.clone(), tool_calls_json, None)
                    .await;
            }
            Message::Tool {
                content,
                tool_call_id,
            } => {
                self.conversation_store
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
}
