// https://bot.q.qq.com/wiki/develop/api-v2/

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

const API_BASE: &str = "https://api.bot.qq.com";

const OP_DISPATCH: u8 = 0;
const OP_HEARTBEAT: u8 = 1;
const OP_IDENTIFY: u8 = 2;
// const OP_RESUME: u8 = 6;
// const OP_RECONNECT: u8 = 7;
// const OP_INVALID_SESSION: u8 = 9;
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
    last_seq: Option<u32>,
}

impl Bot {
    pub fn new() -> Self {
        Self { last_seq: None }
    }

    pub async fn run(&mut self, client: &Client, app_id: &str, client_secret: &str) {
        let mut tokens = TokenManager::new(app_id, client_secret);
        let access_token = tokens.get_token(client).await;
        let mut api = BotApi::new(client.clone(), tokens);

        let gateway_url = api.get_gateway_url().await;
        let (stream, _response) = tokio_tungstenite::connect_async(gateway_url).await.unwrap();
        let (mut write, mut read) = stream.split();

        // 首条消息必定是 Hello
        let interval: Option<u64> = match read.next().await {
            Some(Ok(Message::Text(s))) => {
                let payload = serde_json::from_str::<WsEvent>(&s).ok().unwrap();
                if payload.op == OP_HELLO {
                    let data = serde_json::from_value::<HelloData>(payload.d).ok().unwrap();
                    Some(data.heartbeat_interval)
                } else {
                    None
                }
            }
            _ => None,
        };
        // 登录鉴权
        let identify_cmd = WsCommand {
            op: OP_IDENTIFY,
            d: serde_json::json!(&IdentifyData {
                token: format!("QQBot {access_token}"),
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
                                            }
                                            "C2C_MESSAGE_CREATE" => {
                                                println!("[bot.run]dispatch C2C_MESSAGE_CREATE: {}", payload.d);
                                                let user_openid = payload.d["author"]["user_openid"].as_str().unwrap();
                                                let msg_id = payload.d["id"].as_str().unwrap();
                                                let content = payload.d["content"].as_str().unwrap();
                                                api.send_user_msg(user_openid, msg_id, content).await;
                                            }
                                            _ => {}
                                        }
                                    }
                                    OP_HEARTBEAT_ACK => {
                                        println!("[bot.run]read heartbeat ack")
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {
                            println!("{:?}", msg);
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

struct BotApi {
    client: Client,
    tokens: TokenManager,
}

impl BotApi {
    pub fn new(client: Client, tokens: TokenManager) -> Self {
        BotApi { client, tokens }
    }

    async fn auth_header(&mut self) -> String {
        let access_token = self.tokens.get_token(&self.client).await;
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

    async fn send_user_msg(&mut self, user_openid: &str, msg_id: &str, content: &str) {
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

struct TokenManager {
    state: Option<TokenState>,
    app_id: String,
    client_secret: String,
}

struct TokenState {
    token: String,
    expires_at: Instant,
}

impl TokenManager {
    pub fn new(app_id: &str, client_secret: &str) -> Self {
        Self {
            state: None,
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

    pub async fn get_token(&mut self, client: &Client) -> String {
        if self.is_expired() {
            let (access_token, expires_in) =
                get_access_token(client, &self.app_id, &self.client_secret).await;
            self.state = Some(TokenState {
                token: access_token,
                expires_at: Instant::now() + Duration::from_secs(expires_in),
            });
        }
        return self.state.as_ref().unwrap().token.clone();
    }
}

pub async fn get_access_token(client: &Client, app_id: &str, client_secret: &str) -> (String, u64) {
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
