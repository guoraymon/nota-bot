use chrono::Utc;
use sea_orm::{ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::Value;

use crate::{
    agent::Attachment,
    entities::{conversation, message},
    llm::{self, ContentPart, ImageUrl, Message},
};

pub struct Store {
    pub db: DatabaseConnection,
}

impl Store {
    pub async fn new_conversation(&self) -> i64 {
        let conversation = conversation::Entity::insert(conversation::ActiveModel {
            created_at: ActiveValue::Set(Utc::now().timestamp()),
            ..Default::default()
        })
        .exec(&self.db)
        .await
        .unwrap();
        conversation.last_insert_id
    }

    pub async fn get_last_conversation(&self) -> Option<conversation::Model> {
        conversation::Entity::find()
            .order_by_id_desc()
            .one(&self.db)
            .await
            .unwrap()
    }

    pub async fn get_messages(&mut self, conv_id: i64) -> Vec<Message> {
        let db_messages = message::Entity::find()
            .filter(message::Column::ConversationId.eq(conv_id))
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
                    content: match db_message
                        .payload
                        .as_ref()
                        .and_then(|p| p.get("attachments"))
                        .and_then(|v| serde_json::from_value::<Vec<Attachment>>(v.clone()).ok())
                    {
                        Some(attachments) if !attachments.is_empty() => {
                            let mut parts = vec![ContentPart::Text {
                                text: db_message.content.unwrap_or_default(),
                            }];
                            for attachment in attachments {
                                parts.push(ContentPart::ImageUrl {
                                    image_url: ImageUrl {
                                        url: attachment.get_url(),
                                        detail: "auto".to_owned(),
                                    },
                                });
                            }
                            llm::UserContent::Parts(parts)
                        }
                        _ => llm::UserContent::Text(db_message.content.unwrap_or_default()),
                    },
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

    pub async fn insert_message(
        &mut self,
        conv_id: i64,
        role: &str,
        content: Option<String>,
        payload: Option<Value>,
    ) {
        message::Entity::insert(message::ActiveModel {
            conversation_id: ActiveValue::Set(conv_id),
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
