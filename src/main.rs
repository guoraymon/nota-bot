mod chat_completions;
mod db;
mod skills;
mod tools;

use crate::{
    chat_completions::{FinishReason::ToolCalls, Function, Message, Request, Response, Tool},
    db::DbMessage,
    tools::{Bash, EditFile, Glob, LoadSkill, ReadFile, ToolHandler, WriteFile},
};
use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};

const LLM_URL: &str = "https://api.deepseek.com/chat/completions";

#[tokio::main]
async fn main() {
    let user_msg = std::env::args().nth(1).expect("send a message");
    let api_key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set");

    let nota_agent_home = dirs::home_dir().unwrap().join(".nota-agent");
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
                    skills::list_skills(&dirs::home_dir().unwrap().join(".nota-agent")),
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

    let request = Request {
        model: conversation.get("model"),
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

    let client = Client::new();
    agent_loop(client, api_key, request, conv_id, &mut conn).await;
}

async fn agent_loop(
    client: Client,
    api_key: String,
    mut request: Request,
    conv_id: i64,
    conn: &mut SqliteConnection,
) {
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
            let tool_calls_json = choice
                .message
                .tool_calls
                .as_ref()
                .map(|tc| serde_json::to_string(&tc).unwrap());
            sqlx::query(
        "INSERT INTO messages (conversation_id, role, content, tool_calls, created_at) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(conv_id)
                .bind("assistant")
                .bind(choice.message.content.clone())
                .bind(tool_calls_json)
                .bind(Utc::now().timestamp())
                .execute(&mut *conn)
                .await
                   .unwrap();

            // If the model is done, we're done.
            if choice.finish_reason != ToolCalls {
                return;
            }

            if let Some(tool_calls) = &choice.message.tool_calls {
                for tool_call in tool_calls {
                    let args: Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
                    let result = match tool_call.function.name.as_str() {
                        "bash" => Bash.run(&args),
                        "read_file" => ReadFile.run(&args),
                        "write_file" => WriteFile.run(&args),
                        "edit_file" => EditFile.run(&args),
                        "glob" => Glob.run(&args),
                        "load_skill" => LoadSkill.run(&args),
                        other => format!("unknown tool: {other}"),
                    };
                    request.messages.push(Message::Tool {
                        content: result.clone(),
                        tool_call_id: tool_call.id.clone(),
                    });
                    sqlx::query(
                    "INSERT INTO messages (conversation_id, role, content, tool_call_id, created_at) VALUES (?, ?, ?, ?, ?)",
                            )
                            .bind(conv_id)
                            .bind("tool")
                            .bind(result.clone())
                            .bind(tool_call.id.clone())
                            .bind(Utc::now().timestamp())
                            .execute(&mut *conn)
                            .await
                            .unwrap();
                }
            };
        }
    }
}
