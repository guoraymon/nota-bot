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
const OP_RESUME: u8 = 6;
const OP_RECONNECT: u8 = 7;
const OP_INVALID_SESSION: u8 = 9;
const OP_HELLO: u8 = 10;
const OP_HEARTBEAT_ACK: u8 = 11;

#[derive(Deserialize)]
struct WsEvent {
    id: Option<String>,
    op: u8,
    s: Option<u32>,
    t: Option<String>,
    d: Value,
}

#[derive(Serialize)]
#[serde(tag = "op", content = "d")]
enum WsCommand {
    Identify(IdentifyData),
    Heartbeat(Option<u32>),
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
        let access_token = TokenManager::new(app_id.to_string(), client_secret.to_string())
            .get_token(client)
            .await;
        let gateway_url = get_gateway_url(client, &access_token).await;
        let (stream, response) = tokio_tungstenite::connect_async(gateway_url).await.unwrap();
        let (mut write, mut read) = stream.split();

        // 首条消息必定是 Hello
        let hb_interval: Option<u64> = match read.next().await {
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
        // 心跳
        let mut hb_interval = tokio::time::interval(Duration::from_millis(hb_interval.unwrap()));
        hb_interval.tick().await;

        // 登录鉴权
        let identify_command = WsCommand::Identify(IdentifyData {
            token: format!("QQBot {access_token}"),
            intents: 0 | (1 << 25),
            shard: (0, 1),
            properties: ClientProperties {
                os: "".into(),
                browser: "".into(),
                device: "".into(),
            },
        });
        write
            .send(Message::Text(
                serde_json::to_string(&identify_command).unwrap().into(),
            ))
            .await
            .unwrap();

        loop {
            tokio::select! {
                msg = read.next() => {
                     match msg {
                        Some(Ok(Message::Text(s))) => {
                            println!("{:?}", s);
                            if let Ok(payload) = serde_json::from_str::<WsEvent>(&s) {
                                match payload.op {
                                    OP_DISPATCH => {
                                        self.last_seq = payload.s;
                                        match payload.t.unwrap().as_str() {
                                            "READY" => {
                                                println!("READY");
                                            }
                                            "C2C_MESSAGE_CREATE" => {
                                                println!("C2C_MESSAGE_CREATE: {}", payload.d);
                                            }
                                            _ => {}
                                        }
                                    }
                                    OP_HEARTBEAT_ACK => {}
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ = hb_interval.tick() => {
                    let cmd = WsCommand::Heartbeat(self.last_seq);
                    write.send(serde_json::to_string(&cmd).unwrap().into()).await.unwrap();
                }
            }
        }
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
    expores_at: Instant,
}

impl TokenManager {
    pub fn new(app_id: String, client_secret: String) -> Self {
        Self {
            state: None,
            app_id: app_id,
            client_secret: client_secret,
        }
    }

    pub fn is_expired(&self) -> bool {
        match &self.state {
            None => true,
            Some(state) => Instant::now() >= state.expores_at - REFRESH_MARGIN,
        }
    }

    pub async fn get_token(&mut self, client: &Client) -> String {
        if self.is_expired() {
            let (access_token, expires_in) =
                get_access_token(client, &self.app_id, &self.client_secret).await;
            self.state = Some(TokenState {
                token: access_token,
                expores_at: Instant::now()
                    + Duration::from_secs(expires_in.parse::<u64>().unwrap()),
            });
        }
        return self.state.as_ref().unwrap().token.clone();
    }
}

pub async fn get_access_token(
    client: &Client,
    app_id: &str,
    client_secret: &str,
) -> (String, String) {
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
    return (rep.access_token, rep.expires_in);
}

pub async fn get_gateway_url(client: &Client, access_token: &str) -> String {
    #[derive(Deserialize)]
    struct Rep {
        url: String,
    }

    let rep: Rep = client
        .get(format!("{API_BASE}/gateway"))
        .bearer_auth(format!("QQBot {access_token}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    return rep.url;
}
