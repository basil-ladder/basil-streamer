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
    StartedReplay(String, serde_json::Value),
    Next5Games(Vec<serde_json::Value>),
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    replay_base_url: String,
    twitch: TwitchConfig,
}

#[derive(Serialize, Deserialize, Default)]
struct TwitchConfig {
    channel: String,
    bot_name: String,
    oauth_token: String,
}

async fn twitch_bot(mut rx: Receiver<BasilMessage>, config: Arc<Config>) {
    if !config.twitch.bot_name.is_empty() {
        let client_config = ClientConfig::new_simple(StaticLoginCredentials::new(
            config.twitch.bot_name.clone(),
            Some(config.twitch.oauth_token.clone()),
        ));
        let (_, client) =
            TwitchIRCClient::<TCPTransport, StaticLoginCredentials>::new(client_config);
        while let Ok(message) = rx.recv().await {
            if let BasilMessage::StartedReplay(url, _) = message {
                client.say(config.twitch.channel.clone(), url).await.ok();
            }
        }
        client.join(config.twitch.channel.clone());
    }
}

async fn load_config() -> Result<Config, String> {
    tokio::fs::read("config.toml")
        .await
        .map_err(|e| format!("config.toml required: {}", e))
        .and_then(|data| {
            toml::from_slice(&data).map_err(|e| format!("config.toml could not be parsed: {}", e))
        })
        .map_err(|e| {
            panic!(format!(
                "Could not load config: {}\nNeed a valid config? Here, have one:\n{}",
                e,
                toml::to_string(&Config::default()).unwrap()
            ))
        })
}

async fn serve(broadcast_tx: broadcast::Sender<BasilMessage>) {
    let fs = warp::fs::dir("site");
    let rx = warp::any().map(move || broadcast_tx.clone().subscribe());
    let ws = warp::path("service").and(warp::ws()).and(rx).map(
        |ws: warp::ws::Ws, mut rx: Receiver<BasilMessage>| {
            ws.on_upgrade(move |websocket| async move {
                let (mut tx, _) = websocket.split();
                while let Ok(message) = rx.recv().await {
                    let json = serde_json::to_string(&message).unwrap();
                    tx.send(Message::text(json)).await.ok();
                }
            })
        },
    );
    warp::serve(fs.or(ws)).run(([127, 0, 0, 1], 8080)).await
}

async fn replay_runner(tx: broadcast::Sender<BasilMessage>, config: Arc<Config>) {
    let mut game_queue = vec![];
    loop {
        let entries = fs::read_dir(REPLAYS_DIR).unwrap_or_else(|_| {
            println!("'{}' directory is missing. I'll create one for you! Replays placed there will automatically be scheduled for playing, and will be deleted(!!) afterwards.", REPLAYS_DIR);
            fs::create_dir(REPLAYS_DIR).unwrap_or_else(|_| panic!("Could not create {}", REPLAYS_DIR));
            fs::read_dir(REPLAYS_DIR).unwrap_or_else(|_| panic!("'{}' still missing, please create it.", REPLAYS_DIR))
        })
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()
            .unwrap();

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
            .map(|x| process::Command::new("./ReplayInfo").arg(x).output())
            .collect();
        let replay_infos = futures::future::join_all(next_5_games).await;
        let next_5_games: Result<Vec<_>, String> = replay_infos
            .iter()
            .map(|x| {
                x.as_ref().map_err(|e| format!("{}", e)).and_then(|output| {
                    serde_json::from_slice(&output.stdout).map_err(|e| format!("{}", e))
                })
            })
            .collect();
        match next_5_games {
            Ok(next_5_games) => {
                let info_of_replay_to_play = next_5_games.first().cloned();

                tx.send(BasilMessage::Next5Games(next_5_games)).ok();

                if let (Some(current_replay), Some(replay_file)) =
                    (info_of_replay_to_play, game_queue.first())
                {
                    let url_suffix_candidate =
                        &*replay_file.iter().last().unwrap().to_string_lossy();
                    let (replay_file, file_name) = if replay_file.extension().is_some() && replay_file.extension().unwrap() == "rep" {
                        (
                            replay_file.clone(),
                            replay_file
                                .file_name()
                                .unwrap()
                                .to_string_lossy()
                                .to_string(),
                        )
                    } else {
                        let decoded = base64::decode(url_suffix_candidate).unwrap();
                        let url_suffix = String::from_utf8(decoded).unwrap();

                        let mut rename_path = replay_file.to_path_buf();
                        rename_path.pop();
                        rename_path.push("next_replay.rep");
                        std::fs::rename(replay_file, &rename_path).unwrap();
                        (rename_path, url_suffix)
                    };
                    let url_suffix = url::form_urlencoded::byte_serialize(&file_name.as_bytes()).collect::<String>();
                    let url = config.replay_base_url.clone() + &url_suffix;
                    tx.send(BasilMessage::StartedReplay(url, current_replay))
                        .ok();
                    let process = process::Command::new("./ReplayViewer")
                        .env("BWAPI_CONFIG_AUTO_MENU__MAP", &replay_file)
                        .spawn();
                    if let Ok(process) = process {
                        process.await.expect("Could not execute ReplayViewer");
                        fs::remove_file(&replay_file).ok();
                        game_queue.remove(0);
                        tx.send(BasilMessage::GameCompleted).ok();
                    } else {
                        println!("Could not execute ReplayViewer - please check if its present and executable. Pausing for 15 seconds");
                        tokio::time::delay_for(tokio::time::Duration::from_secs(15)).await;
                    }
                } else {
                    println!(
                        "No replays found (retrying in 5 seconds). Copy some into '{}'.",
                        REPLAYS_DIR
                    );
                    tokio::time::delay_for(tokio::time::Duration::from_secs(5)).await;
                }
            }
            Err(e) => {
                println!("Could not execute ReplayInfo: {}  - please check if its present and executable. Pausing for 15 seconds", e);
                tokio::time::delay_for(tokio::time::Duration::from_secs(15)).await;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("BASIL Replay Control Program {}", VERSION);
    let config = Arc::new(load_config().await?);
    let (broadcast_tx, rx) = broadcast::channel(5);

    tokio::spawn(twitch_bot(rx, config.clone()));
    let replayer = tokio::spawn(replay_runner(broadcast_tx.clone(), config));
    let http_server = tokio::spawn(serve(broadcast_tx));

    let (_, _) = tokio::join!(replayer, http_server);
    Ok(())
}
