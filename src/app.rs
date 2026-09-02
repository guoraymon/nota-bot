use std::time::Duration;

use base64::Engine;
use futures_util::future::join_all;
use serde_json::json;

use crate::{
    agent::{Agent, AgentMessage, Attachment},
    bot::{BotApi, C2CMESSAGE},
    store::Store,
};

pub struct App {
    pub store: Store,
    pub conv_id: i64,
    pub agent: Agent,
    pub bot_api: BotApi,
}

impl App {
    pub async fn handle_msg(&mut self, msg: C2CMESSAGE) {
        let attachments = if let Some(attachments) = msg.attachments.as_ref() {
            Some(
                join_all(
                    attachments
                        .iter()
                        .filter(|attachment| {
                            matches!(
                                attachment.content_type.as_str(),
                                "image/jpeg" | "image/png" | "image/gif"
                            )
                        })
                        .map(async |attachment| {
                            let response = reqwest::get(attachment.url.to_owned()).await.ok()?;
                            let bytes = response.bytes().await.ok()?;
                            let base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                            Some(Attachment {
                                content_type: attachment.content_type.to_owned(),
                                data: base64,
                            })
                        }),
                )
                .await
                .into_iter()
                .flatten()
                .collect(),
            )
        } else {
            None
        };
        self.store
            .insert_message(
                self.conv_id,
                "user",
                Some(msg.content.to_string()),
                if attachments.is_some() {
                    Some(json!({ "attachments": attachments }))
                } else {
                    None
                },
            )
            .await;
        let mut last_content = None;
        let result = match self.agent.send(&msg.content, attachments.as_ref()).await {
            Ok(events) => events,
            Err(e) => {
                self.bot_api
                    .send_user_msg(&msg.author.user_openid, &msg.id, &e)
                    .await;
                return;
            }
        };

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
                    self.store
                        .insert_message(
                            self.conv_id,
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
                        } else if let Some(prompt_tokens_details) = usage.prompt_tokens_details {
                            cached_tokens += prompt_tokens_details.cached_tokens;
                        }
                    }

                    total_elapsed += elapsed;
                }
                AgentMessage::Tool {
                    content,
                    tool_call_id,
                } => {
                    self.store
                        .insert_message(
                            self.conv_id,
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
                "{last_content}\n\n> {:.1} tok/s ↑{prompt_tokens} ↓{completion_tokens} CH{:.2}%",
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
            self.bot_api
                .send_user_msg(&msg.author.user_openid, &msg.id, &content)
                .await;
        }
    }
}
