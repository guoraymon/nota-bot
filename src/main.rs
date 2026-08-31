mod agent;
mod bot;
mod entities;
mod llm;
mod skills;
mod tools;

use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    agent::{Agent, AgentMessage},
    bot::{Bot, BotApi, C2CMESSAGE, TokenManager},
    entities::{conversation, message},
    llm::Message,
};
use chrono::Utc;
use reqwest::Client;
use sea_orm::{
    ActiveValue, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DbBackend,
    EntityTrait, QueryFilter, Schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

#[derive(Deserialize)]
struct Config {
    app_id: String,
    client_secret: String,
    default_provider: String,
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

    let db = Database::connect(format!(
        "sqlite://{}?mode=rwc",
        nota_agent_home.join("default.db").display()
    ))
    .await
    .unwrap();

    // init table
    let schema = Schema::new(DbBackend::Sqlite);
    db.execute(
        schema
            .create_table_from_entity(conversation::Entity)
            .if_not_exists(),
    )
    .await
    .unwrap();
    db.execute(
        schema
            .create_table_from_entity(message::Entity)
            .if_not_exists(),
    )
    .await
    .unwrap();

    let conversation = conversation::Entity::find()
        .order_by_id_desc()
        .one(&db)
        .await
        .unwrap();
    if conversation.is_none() {
        conversation::Entity::insert(conversation::ActiveModel {
            created_at: ActiveValue::Set(Utc::now().timestamp()),
            ..Default::default()
        })
        .exec(&db)
        .await
        .unwrap();
        let conversation = conversation::Entity::find().one(&db).await.unwrap();

        let avaliable_skills = skills::find_skills(&nota_agent_home.join("skills"))
            .iter()
            .map(|skill| {
                format!(
                    "<skill>
                            <name>{}</name>
                            <description>{}</description>
                            <location>{}</location>
                        </skill>",
                    skill.name, skill.description, skill.location
                )
            })
            .collect::<Vec<String>>()
            .join("\n");
        let skill_prompt = format!(
            "The following skills provide specialized instructions...
                Use the read tool to load a skill's file when the task matches its description.

                <available_skills>
                    {avaliable_skills}
                </available_skills>"
        );
        let system_prompt = format!("You are a helpful assistant.\n{skill_prompt}");
        if let Some(conversation) = conversation {
            message::Entity::insert(message::ActiveModel {
                conversation_id: ActiveValue::Set(conversation.id),
                role: ActiveValue::Set("system".to_owned()),
                content: ActiveValue::Set(Some(system_prompt.to_owned())),
                payload: ActiveValue::NotSet,
                created_at: ActiveValue::Set(Utc::now().timestamp()),
                ..Default::default()
            })
            .exec(&db)
            .await
            .unwrap();
        } else {
            eprintln!("conversation init error");
        }
    }

    let conv_id: i64 = conversation.unwrap().id;

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

    let mut conversation_store = ConversationStore { db, conv_id };
    let mut agent: Agent = Agent::new(
        default_provider.url.clone(),
        default_provider.key.clone(),
        default_model.model.clone(),
        conversation_store.get().await,
    );

    let (tx, mut rx) = mpsc::channel::<C2CMESSAGE>(100);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let images = msg.attachments.as_ref().map(|attachments| {
                attachments
                    .iter()
                    .filter(|attachment| {
                        matches!(
                            attachment.content_type.as_str(),
                            "image/jpeg" | "image/png" | "image/gif"
                        )
                    })
                    .map(|attachment| attachment.url.as_str())
                    .collect()
            });
            conversation_store
                .append("user", Some(msg.content.to_string()), None)
                .await;
            let mut last_content = None;
            let result = agent.send(&msg.content, images).await;

            let mut prompt_tokens = 0;
            let mut completion_tokens = 0;
            let mut cached_tokens = 0;
            let mut total_elapsed = Duration::new(0, 0);

            for message in result {
                match message {
                    AgentMessage::Assistant {
                        content,
                        tool_calls,
                        usage,
                        elapsed,
                    } => {
                        conversation_store
                            .append(
                                "assistant",
                                Some(content.clone()),
                                Some(json!({"tool_calls": tool_calls})),
                            )
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

                        total_elapsed += elapsed;
                    }
                    AgentMessage::Tool {
                        content,
                        tool_call_id,
                    } => {
                        conversation_store
                            .append(
                                "tool",
                                Some(content.clone()),
                                Some(json!({"tool_call_id": tool_call_id})),
                            )
                            .await;
                    }
                }
            }

            if let Some(last_content) = last_content {
                let content = format!(
                    "{last_content}\n\nTPS {:.1} tok/s ↑{prompt_tokens} ↓{completion_tokens} CH{:.2}%",
                    if total_elapsed.as_secs_f64() > 0.0 {
                        completion_tokens as f64 / total_elapsed.as_secs_f64()
                    } else {
                        0.0
                    },
                    if cached_tokens > 0 {
                        cached_tokens as f64 / prompt_tokens as f64 * 100f64
                    } else {
                        0f64
                    }
                );
                bot_api
                    .send_user_msg(&msg.author.user_openid, &msg.id, &content)
                    .await;
            }
        }
    });

    bot.run(tx).await;

    return;
}

struct ConversationStore {
    db: DatabaseConnection,
    conv_id: i64,
}

impl ConversationStore {
    pub async fn get(&mut self) -> Vec<Message> {
        let db_messages = message::Entity::find()
            .filter(message::Column::ConversationId.eq(self.conv_id))
            .all(&self.db)
            .await
            .unwrap();
        db_messages
            .into_iter()
            .map(|db_message| match db_message.role.as_str() {
                "system" => Message::System {
                    content: db_message.content.unwrap_or_default(),
                    name: None,
                },
                "user" => Message::User {
                    content: llm::UserContent::Text(db_message.content.unwrap_or_default()),
                    name: None,
                },
                "assistant" => Message::Assistant {
                    content: db_message.content,
                    name: None,
                    tool_calls: db_message
                        .payload
                        .as_ref()
                        .and_then(|p| p.get("tool_calls"))
                        .and_then(|v| v.as_array())
                        .and_then(|arr| {
                            serde_json::from_value(Value::Array(arr.to_vec())).unwrap()
                        }),
                },
                "tool" => Message::Tool {
                    content: db_message.content.unwrap_or_default(),
                    tool_call_id: db_message
                        .payload
                        .as_ref()
                        .and_then(|p| p.get("tool_call_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                },
                _ => {
                    panic!("Unknown role: {}", db_message.role)
                }
            })
            .collect::<Vec<Message>>()
    }

    pub async fn append(&mut self, role: &str, content: Option<String>, payload: Option<Value>) {
        message::Entity::insert(message::ActiveModel {
            conversation_id: ActiveValue::Set(self.conv_id),
            role: ActiveValue::Set(role.to_owned()),
            content: ActiveValue::Set(content),
            payload: ActiveValue::Set(payload),
            created_at: ActiveValue::Set(Utc::now().timestamp()),
            ..Default::default()
        })
        .exec(&self.db)
        .await
        .unwrap();
    }
}
