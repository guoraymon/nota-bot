mod agent;
mod app;
mod bot;
mod config;
mod entities;
mod llm;
mod skills;
mod store;
mod tools;

use std::sync::Arc;

use crate::{
    agent::Agent,
    app::App,
    bot::{Bot, BotApi, C2CMESSAGE, TokenManager},
    config::Config,
    entities::{conversation, message},
    store::Store,
};
use reqwest::Client;
use sea_orm::{ConnectionTrait, Database, DbBackend, Schema};
use tokio::sync::mpsc;

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

    let content = std::fs::read_to_string(nota_agent_home.join("config.json")).unwrap();
    let config: Config = serde_json::from_str(&content).unwrap();

    let client = Client::new();
    let token_manager = Arc::new(TokenManager::new(
        &client,
        &config.app_id,
        &config.client_secret,
    ));
    let mut bot = Bot::new(&client, token_manager.clone());
    let bot_api = BotApi::new(&client, token_manager.clone());

    let mut store = Store { db };
    let conversation = store.get_last_conversation().await;
    let conv_id = if let Some(conversation) = conversation {
        conversation.id
    } else {
        let conv_id = store.new_conversation().await;
        store
            .insert_message(
                conv_id,
                "system",
                Some(App::get_system_prompt(&nota_agent_home)),
                None,
            )
            .await;
        conv_id
    };

    let default_provider = config
        .get_default_provider()
        .expect("get default provider error");
    let default_model = config.get_default_model().expect("get default model error");
    let agent: Agent = Agent::new(
        default_provider.url.clone(),
        default_provider.key.clone(),
        default_model.model.clone(),
        store.get_messages(conv_id).await,
    );

    let mut app = App {
        home_path: nota_agent_home,
        store,
        conv_id,
        bot_api,
        agent,
        config,
    };
    let (tx, mut rx) = mpsc::channel::<C2CMESSAGE>(100);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            app.handle_msg(msg).await;
        }
    });

    bot.run(tx).await;

    return;
}
