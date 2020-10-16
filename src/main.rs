use actix_web::{App, HttpServer};
use actix_web_static_files;
use crossbeam_channel::unbounded;
use rand::seq::IteratorRandom;
use rand::thread_rng;
use serde::Serialize;
use std::collections::HashMap;
use std::{fs, io, net, process, thread, time};
use tungstenite::server::accept;
use tungstenite::Message;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPLAYS_DIR: &str = "replay_queue";

#[derive(Serialize)]
enum BasilMessage {
    GameCompleted,
    StartedReplay(String),
    Next5Games(Vec<String>),
}

/// A WebSocket echo server
fn main() {
    println!("BASIL Replay Control Program {}", VERSION);
    let (tx, rx) = unbounded();
    thread::spawn(move || {
        let mut rng = thread_rng();
        let mut game_queue = vec![];
        loop {
            let entries = fs::read_dir(REPLAYS_DIR).unwrap_or_else(|_| {
                println!("'{}' directory is missing. I'll create one for you! Replays placed there will automatically be scheduled for playing, and will be deleted(!!) afterwards.", REPLAYS_DIR);
                fs::create_dir(REPLAYS_DIR).unwrap_or_else(|_| panic!("Could not create {}", REPLAYS_DIR));
                fs::read_dir(REPLAYS_DIR).unwrap_or_else(|_| panic!("'{}' still missing, please create it.", REPLAYS_DIR))
            })
                .map(|res| res.map(|e| e.path()))
                .collect::<Result<Vec<_>, io::Error>>().unwrap();

            let mut next_items = entries
                .iter()
                .filter(|it| game_queue.iter().find(|x| x == it).is_none())
                .cloned()
                .choose_multiple(&mut rng, 5 - game_queue.len());
            game_queue.append(&mut next_items);
            let next_5_games: Vec<_> = game_queue
                .iter()
                .map(|x| x.iter().last().unwrap().to_string_lossy().to_string())
                .collect();
            tx.send(BasilMessage::Next5Games(next_5_games)).unwrap();
            if let Some(current_replay) = game_queue.first() {
                tx.send(BasilMessage::StartedReplay(
                    current_replay
                        .iter()
                        .last()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                ))
                .unwrap();
                process::Command::new("./ReplayViewer")
                    .env("BWAPI_CONFIG_AUTO_MENU__MAP", &current_replay)
                    .output()
                    .expect("Could not execute ReplayViewer");
                fs::remove_file(&current_replay).ok();
                game_queue.swap_remove(0);
                tx.send(BasilMessage::GameCompleted).unwrap();
            } else {
                println!(
                    "No replays found (retrying in 5 seconds). Copy some into '{}'.",
                    REPLAYS_DIR
                );
                thread::sleep(time::Duration::from_secs(5));
            }
        }
    });
    thread::spawn(|| {
        let _unused = actix_rt::System::new("basil");
        HttpServer::new(move || {
            let generated = generate();
            App::new().service(actix_web_static_files::ResourceFiles::new("/", generated))
        })
        .bind("127.0.0.1:8080")
        .unwrap()
        .run();
    });
    let server = net::TcpListener::bind("127.0.0.1:9001").unwrap();
    for stream in server.incoming() {
        let rx = rx.clone();
        thread::spawn(move || {
            let stream = stream.unwrap();
            let mut websocket = accept(stream).unwrap();
            loop {
                let message = rx.recv().expect("recv failed");
                websocket
                    .write_message(Message::text(serde_json::to_string(&message).unwrap()))
                    .unwrap();
                /*                let msg = websocket.read_message().unwrap();
                // We do not want to send back ping/pong messages.
                if msg.is_binary() || msg.is_text() {
                    websocket.write_message(msg).unwrap();
                }
                */
            }
        });
    }
}
