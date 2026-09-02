use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use base64::Engine;
use futures_util::future::join_all;
use serde_json::json;

use crate::{
    agent::{Agent, AgentMessage, Attachment},
    bot::{BotApi, C2CMESSAGE},
    skills,
    store::Store,
};

pub struct App {
    pub home_path: PathBuf,
    pub store: Store,
    pub conv_id: i64,
    pub agent: Agent,
    pub bot_api: BotApi,
}

impl App {
    pub fn get_system_prompt(home_path: &Path) -> String {
        let skill_prompt = format!(
            "The following skills provide specialized instructions...\n\
            Use the read tool to load a skill's file when the task matches its description.\n\
            <available_skills>\n\
            {}\n\
            </available_skills>",
            skills::find_skills(&home_path.join("skills"))
                .iter()
                .map(|skill| {
                    format!(
                        "<skill>\n  <name>{}</name>\n  <description>{}</description>\n  <location>{}</location>\n</skill>",
                        skill.name, skill.description, skill.location
                    )
                })
                .collect::<Vec<String>>()
                .join("\n")
        );
        format!("You are a helpful assistant.\n{skill_prompt}")
    }

    pub async fn handle_msg(&mut self, msg: C2CMESSAGE) {
        if msg.content.starts_with('/') {
            match msg.content.as_str() {
                "/new" => {
                    self.conv_id = self.store.new_conversation().await;
                    self.store
                        .insert_message(
                            self.conv_id,
                            "system",
                            Some(App::get_system_prompt(&self.home_path)),
                            None,
                        )
                        .await;
                    self.agent
                        .reset(self.store.get_messages(self.conv_id).await);

                    self.bot_api
                        .send_user_msg(&msg.author.user_openid, &msg.id, "已开启新对话")
                        .await;
                }
                _ => {
                    self.bot_api
                        .send_user_msg(&msg.author.user_openid, &msg.id, "暂不支持的命令")
                        .await;
                }
            }
            return;
        }

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

        let mut steps = 0;
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
                            content.clone(),
                            Some(json!({"tool_calls": tool_calls})),
                        )
                        .await;

                    last_content = content;

                    steps += 1;
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
                "{last_content}\n\n> {steps} steps {:.1} tok/s ↑{prompt_tokens} ↓{completion_tokens} CH{:.2}% ¥{:.4}",
                if total_elapsed.as_secs_f64() > 0.0 {
                    completion_tokens as f64 / total_elapsed.as_secs_f64()
                } else {
                    0.0
                },
                if cached_tokens > 0 {
                    cached_tokens as f64 / prompt_tokens as f64 * 100f64
                } else {
                    0f64
                },
                (cached_tokens as f64 * 0.1 / 1_000_000.0)
                    + ((prompt_tokens - cached_tokens) as f64 * 3.0 / 1_000_000.0)
                    + (completion_tokens as f64 * 9.0 / 1_000_000.0)
            );
            self.bot_api
                .send_user_msg(&msg.author.user_openid, &msg.id, &content)
                .await;
        }
    }
}
