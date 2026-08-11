// https://bot.q.qq.com/wiki/develop/api-v2/

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use rand::RngExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::Message;

use crate::IncomingMessage;

const API_BASE: &str = "https://api.bot.qq.com";

const OP_DISPATCH: u8 = 0;
const OP_HEARTBEAT: u8 = 1;
const OP_IDENTIFY: u8 = 2;
const OP_RESUME: u8 = 6;
const OP_RECONNECT: u8 = 7;
const OP_INVALID_SESSION: u8 = 9;
const OP_HELLO: u8 = 10;
const OP_HEARTBEAT_ACK: u8 = 11;

#[derive(Deserialize)]
struct WsEvent {
    _id: Option<String>,
    op: u8,
    s: Option<u32>,
    t: Option<String>,
    d: Value,
}

#[derive(Debug, Serialize)]
struct WsCommand {
    op: u8,
    d: Value,
}

#[derive(Serialize, Deserialize)]
struct HelloData {
    heartbeat_interval: u64,
}

#[derive(Serialize, Deserialize)]
struct IdentifyData {
    token: String,
    intents: u64,
    shard: (u8, u8),
    properties: ClientProperties,
}

#[derive(Serialize, Deserialize)]
struct ClientProperties {
    #[serde(rename = "$os")]
    os: String,
    #[serde(rename = "$browser")]
    browser: String,
    #[serde(rename = "$device")]
    device: String,
}

pub struct Bot {
    api: BotApi,
    session_id: Option<String>,
    last_seq: Option<u32>,
}

impl Bot {
    pub fn new(client: &Client, app_id: &str, client_secret: &str) -> Self {
        let tokens = TokenManager::new(client, app_id, client_secret);
        let api = BotApi::new(client, tokens);
        Self {
            api,
            session_id: None,
            last_seq: None,
        }
    }

    pub async fn run(&mut self, tx: Sender<IncomingMessage>) {
        let mut attempt = 0;
        let gateway_url = self.api.get_gateway_url().await;
        loop {
            if attempt > 0 {
                let half: u64 = (2u64).saturating_pow(attempt).min(30) * 1000 / 2;
                let delay = Duration::from_millis(half + rand::rng().random_range(0..=half));
                println!("[bot.run]sleep {:?}", delay);
                tokio::time::sleep(delay).await;
            }

            let (stream, _) = match tokio_tungstenite::connect_async(gateway_url.clone()).await {
                Ok(s) => s,
                Err(e) => {
                    println!("Failed to connect to gateway: {}", e);
                    attempt += 1;
                    continue;
                }
            };
            let (mut write, mut read) = stream.split();

            // 首条消息必定是 Hello
            let interval = async {
                let msg = read.next().await?.ok()?;
                let Message::Text(text) = msg else {
                    return None;
                };
                let payload = serde_json::from_str::<WsEvent>(&text).ok()?;
                if payload.op != OP_HELLO {
                    return None;
                }
                let data = serde_json::from_value::<HelloData>(payload.d).ok()?;
                Some(data.heartbeat_interval)
            }
            .await;
            if interval.is_none() {
                attempt += 1;
                continue;
            }

            if self.session_id.is_none() {
                // 登录鉴权
                let identify_cmd = WsCommand {
                    op: OP_IDENTIFY,
                    d: serde_json::json!(&IdentifyData {
                        token: self.api.auth_header().await,
                        intents: 0 | (1 << 25),
                        shard: (0, 1),
                        properties: ClientProperties {
                            os: "".into(),
                            browser: "".into(),
                            device: "".into(),
                        },
                    }),
                };
                let identify_cmd_string = serde_json::to_string(&identify_cmd).unwrap();
                println!("[bot.run]write identify: {identify_cmd_string}");
                write.send(identify_cmd_string.into()).await.unwrap();
            } else {
                let cmd = WsCommand {
                    op: OP_RESUME,
                    d: json!({
                        "token": self.api.auth_header().await,
                        "session_id": &self.session_id,
                        "seq": &self.last_seq,
                    }),
                };
                let cmd_string = serde_json::to_string(&cmd).unwrap();
                println!("[bot.run]write resume: {cmd_string}");
                write.send(cmd_string.into()).await.unwrap();
            }

            // 心跳
            let mut hb_interval = tokio::time::interval(Duration::from_millis(interval.unwrap()));

            loop {
                tokio::select! {
                    msg = read.next() => {
                         match msg {
                            Some(Ok(Message::Text(s))) => {
                                if let Ok(payload) = serde_json::from_str::<WsEvent>(&s) {
                                    match payload.op {
                                        OP_DISPATCH => {
                                            self.last_seq = payload.s;
                                            match payload.t.unwrap().as_str() {
                                                "READY" => {
                                                    println!("[bot.run]dispatch READY: {}", payload.d);
                                                    let session_id = payload.d["session_id"].as_str().unwrap();
                                                    self.session_id = Some(session_id.to_string());
                                                    attempt = 0;
                                                }
                                                "RESUMED" => {
                                                    println!("[bot.run]dispatch RESUMED");
                                                    attempt = 0;
                                                }
                                                "C2C_MESSAGE_CREATE" => {
                                                    println!("[bot.run]dispatch C2C_MESSAGE_CREATE: {}", payload.d);
                                                    let user_openid = payload.d["author"]["user_openid"].as_str().unwrap();
                                                    let msg_id = payload.d["id"].as_str().unwrap();
                                                    let content = payload.d["content"].as_str().unwrap();
                                                    let _ = tx.send(IncomingMessage {
                                                        content:content.to_owned(),
                                                        user_openid: user_openid.to_owned(),
                                                        msg_id: msg_id.to_owned()
                                                    }).await;
                                                }
                                                _ => {}
                                            }
                                        }
                                        OP_RECONNECT => {
                                            attempt += 1;
                                            break;
                                        }
                                        OP_INVALID_SESSION => {
                                            if payload.d == false {
                                                self.session_id = None;
                                            }
                                            attempt += 1;
                                            break;
                                        }
                                        OP_HEARTBEAT_ACK => {
                                            println!("[bot.run]read heartbeat ack")
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Some(Err(_)) => {
                                println!("[bot.run]read error");
                                attempt += 1;
                                break;
                            }
                            Some(Ok(Message::Close(_))) => {
                                println!("[bot.run]read close");
                                attempt += 1;
                                break;
                            }
                            None => {
                                println!("[bot.run]read none");
                                attempt += 1;
                                break;
                            }
                            _ => {
                                println!("[bot.run]read other: {:?}", msg);
                            }
                        }
                    }
                    _ = hb_interval.tick() => {
                        let cmd = WsCommand{op: OP_HEARTBEAT, d: serde_json::json!(&self.last_seq) };
                        let cmd_string = serde_json::to_string(&cmd).unwrap();
                        println!("[bot.run]write heartbeat: {cmd_string}");
                        write.send(cmd_string.into()).await.unwrap();
                    }
                }
            }
        }
    }
}

pub struct BotApi {
    client: Client,
    tokens: TokenManager,
}

impl BotApi {
    pub fn new(client: &Client, tokens: TokenManager) -> Self {
        BotApi {
            client: client.to_owned(),
            tokens: tokens,
        }
    }

    async fn auth_header(&mut self) -> String {
        let access_token = self.tokens.get_token().await;
        format!("QQBot {access_token}")
    }

    pub async fn get_gateway_url(&mut self) -> String {
        #[derive(Deserialize)]
        struct Rep {
            url: String,
        }

        let rep: Rep = self
            .client
            .get(format!("{API_BASE}/gateway"))
            .header("Authorization", self.auth_header().await)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        return rep.url;
    }

    pub async fn send_user_msg(&mut self, user_openid: &str, msg_id: &str, content: &str) {
        let json = serde_json::json!({
            "content": content,
            "msg_type": 0,
            "msg_id": msg_id,
        });
        println!("send_user_msg: {json}");
        let res = self
            .client
            .post(format!("{API_BASE}/v2/users/{user_openid}/messages"))
            .header("Authorization", self.auth_header().await)
            .json(&json)
            .send()
            .await
            .unwrap();
        let rep: Value = res.json().await.unwrap();
        println!("send_user_msg rep: {rep}")
    }
}

const REFRESH_MARGIN: Duration = Duration::from_secs(60);

pub struct TokenManager {
    state: Option<TokenState>,
    client: Client,
    app_id: String,
    client_secret: String,
}

struct TokenState {
    token: String,
    expires_at: Instant,
}

impl TokenManager {
    pub fn new(client: &Client, app_id: &str, client_secret: &str) -> Self {
        Self {
            state: None,
            client: client.clone(),
            app_id: app_id.to_owned(),
            client_secret: client_secret.to_owned(),
        }
    }

    pub fn is_expired(&self) -> bool {
        match &self.state {
            None => true,
            Some(state) => Instant::now() >= state.expires_at - REFRESH_MARGIN,
        }
    }

    pub async fn get_token(&mut self) -> String {
        if self.is_expired() {
            let (access_token, expires_in) =
                get_access_token(&self.client, &self.app_id, &self.client_secret).await;
            self.state = Some(TokenState {
                token: access_token,
                expires_at: Instant::now() + Duration::from_secs(expires_in),
            });
        }
        return self.state.as_ref().unwrap().token.clone();
    }
}

async fn get_access_token(client: &Client, app_id: &str, client_secret: &str) -> (String, u64) {
    #[derive(Serialize)]
    #[allow(non_snake_case)]
    struct Req<'a> {
        appId: &'a str,
        clientSecret: &'a str,
    }

    #[derive(Deserialize)]
    struct Rep {
        access_token: String,
        expires_in: String,
    }

    let res = client
        .post(format!("{API_BASE}/app/getAppAccessToken"))
        .json(&Req {
            appId: app_id,
            clientSecret: client_secret,
        })
        .send()
        .await
        .unwrap();
    let rep: Rep = res.json().await.unwrap();
    return (rep.access_token, rep.expires_in.parse::<u64>().unwrap());
}
