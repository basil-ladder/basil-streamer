use futures::prelude::*;
use rand::seq::IteratorRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{fs, io};
use tokio::process;
use tokio::sync::broadcast::{self, Receiver};
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::{ClientConfig, TCPTransport, TwitchIRCClient};
use warp::ws::Message;
use warp::Filter;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPLAYS_DIR: &str = "replay_queue";

#[derive(Debug, Serialize, Clone)]
enum BasilMessage {
    GameCompleted,
    StartedReplay(String),
    Next5Games(Vec<String>),
}

#[derive(Deserialize)]
struct Config {
    twitch: TwitchConfig,
}

#[derive(Deserialize)]
struct TwitchConfig {
    channel: String,
    bot_name: String,
    oauth_token: String,
}

async fn connect_to_twitch(mut rx: Receiver<BasilMessage>, config: Arc<Config>) {
    let client_config = ClientConfig::new_simple(StaticLoginCredentials::new(
        config.twitch.bot_name.clone(),
        Some(config.twitch.oauth_token.clone()),
    ));
    let (_, client) = TwitchIRCClient::<TCPTransport, StaticLoginCredentials>::new(client_config);
    /*
    let h = tokio::spawn(async move {
        while let Some(message) = incoming_messages.next().await {
            println!("Received message: {:?}", message);
        }
    });
    */
    while let Ok(message) = rx.recv().await {
        if let BasilMessage::StartedReplay(replay) = message {
            client
                .say(config.twitch.channel.clone(), replay)
                .await
                .unwrap()
        }
    }
    client.join(config.twitch.channel.clone());
    //    h.await.unwrap()
}

async fn load_config() -> Result<Config, String> {
    let config: Config = toml::from_slice(
        &tokio::fs::read("config.toml")
            .await
            .map_err(|e| format!("config.toml required: {}", e))?,
    )
    .unwrap();
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("BASIL Replay Control Program {}", VERSION);
    let config = Arc::new(load_config().await?);
    let (broadcast_tx, rx) = broadcast::channel(5);

    tokio::spawn(connect_to_twitch(rx, config));

    let tx = broadcast_tx.clone();
    let replayer = tokio::spawn(async move {
        let mut game_queue = vec![];
        loop {
            let entries = fs::read_dir(REPLAYS_DIR).unwrap_or_else(|_| {
                println!("'{}' directory is missing. I'll create one for you! Replays placed there will automatically be scheduled for playing, and will be deleted(!!) afterwards.", REPLAYS_DIR);
                fs::create_dir(REPLAYS_DIR).unwrap_or_else(|_| panic!("Could not create {}", REPLAYS_DIR));
                fs::read_dir(REPLAYS_DIR).unwrap_or_else(|_| panic!("'{}' still missing, please create it.", REPLAYS_DIR))
            })
                .map(|res| res.map(|e| e.path()))
                .collect::<Result<Vec<_>, io::Error>>().unwrap();

            let mut next_items = {
                let mut rng = thread_rng();
                entries
                    .iter()
                    .filter(|it| game_queue.iter().find(|x| x == it).is_none())
                    .cloned()
                    .choose_multiple(&mut rng, 5 - game_queue.len())
            };
            game_queue.append(&mut next_items);
            let next_5_games: Vec<_> = game_queue
                .iter()
                .map(|x| x.iter().last().unwrap().to_string_lossy().to_string())
                .collect();
            tx.send(BasilMessage::Next5Games(next_5_games)).ok();
            if let Some(current_replay) = game_queue.first() {
                tx.send(BasilMessage::StartedReplay(
                    current_replay
                        .iter()
                        .last()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                ))
                .ok();
                process::Command::new("./ReplayViewer")
                    .env("BWAPI_CONFIG_AUTO_MENU__MAP", &current_replay)
                    .spawn()
                    .expect("Could not execute ReplayViewer")
                    .await
                    .expect("Could not execute ReplayViewer");
                fs::remove_file(&current_replay).ok();
                game_queue.remove(0);
                tx.send(BasilMessage::GameCompleted).ok();
            } else {
                println!(
                    "No replays found (retrying in 5 seconds). Copy some into '{}'.",
                    REPLAYS_DIR
                );
                tokio::time::delay_for(tokio::time::Duration::from_secs(5)).await;
            }
        }
    });

    let fs = warp::fs::dir("bottom");
    let http_server = tokio::spawn(warp::serve(fs).run(([127, 0, 0, 1], 8080)));
    let rx = warp::any().map(move || broadcast_tx.clone().subscribe());
    let ws = warp::any().and(warp::ws()).and(rx).map(
        |ws: warp::ws::Ws, mut rx: Receiver<BasilMessage>| {
            ws.on_upgrade(move |websocket| async move {
                let (mut tx, _) = websocket.split();
                while let Ok(message) = rx.recv().await {
                    let json = serde_json::to_string(&message).unwrap();
                    tx.send(Message::text(json)).await.unwrap();
                }
            })
        },
    );
    let ws_server = warp::serve(ws).run(([127, 0, 0, 1], 9001));
    let (_, _, _) = tokio::join!(replayer, ws_server, http_server);
    Ok(())
}
