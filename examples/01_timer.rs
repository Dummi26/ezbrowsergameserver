use std::time::Instant;

use async_trait::async_trait;
use ezbrowsergameserver::prelude::*;

// Run `server.sh` and open `0.0.0.0:8080/01_timer.html` to play the game.
// You need at least 2 players to ready up for the game to start.

#[tokio::main]
async fn main() {
    // Host the game's WebSocket on 0.0.0.0:8081
    host::<GlobalState>("0.0.0.0:8081").await;
}

/// Since this is such a small demo, there is no global state.
/// This would usually contain game settings
#[derive(Default)]
struct GlobalState {
    update: bool,
}

#[async_trait]
impl LobbyState for GlobalState {
    // (name, ready)
    type PlayerState = (String, bool);
    fn new() -> Self {
        Self { update: false }
    }
    fn new_player() -> Self::PlayerState {
        ("new player".to_owned(), false)
    }
    async fn player_joined(_id: usize, lobby: &mut Lobby<Self>, _player: PlayerIndex) {
        lobby.state.update = true;
    }
    async fn player_leaving(_id: usize, lobby: &mut Lobby<Self>, _player: PlayerIndex) {
        lobby.state.update = true;
    }
    async fn lobby_update(id: usize, lobby: &mut Lobby<Self>) -> Option<Box<dyn GameState<Self>>> {
        let mut update = lobby.state.update;
        if lobby.reset {
            lobby.reset = false;
            update = true;
            for player in lobby.players_mut() {
                player.data.1 = false;
            }
        }
        for player in lobby.players_mut() {
            while let Some(msg) = player.get_msg().await {
                if msg.starts_with("n") {
                    player.data.0 = msg[1..].to_owned();
                } else if msg == "R1" {
                    player.data.1 = true;
                } else if msg == "R0" {
                    player.data.1 = false;
                }
                update = true;
            }
        }
        if update {
            lobby.state.update = false;
            for player in lobby.player_indices() {
                let players_list = lobby
                    .players()
                    .iter()
                    .map(|p| {
                        if p.data.1 {
                            format!("<b>{}</b><br>", p.data.0)
                        } else {
                            format!("{}<br>", p.data.0)
                        }
                    })
                    .collect::<String>();
                let player = lobby.get_player(player);
                player
                    .send(format!(
                        "=<h1>Welcome to the lobby, {}!</h1><p>Lobby ID: {id:x}</p><p>{}</p><p>{players_list}</p>",
                        player.data.0,
                        if player.data.1 {
                            "<button onclick='con.send(\"R0\")'>Ready!</button>"
                        } else {
                            "<button onclick='con.send(\"R1\")'>ready up</button>"
                        },
                    ))
                    .await;
            }
        }
        if lobby.players().len() >= 2 && lobby.players().iter().all(|v| v.data.1) {
            // all 2+ players are ready, start the game
            return Some(Box::new(TimerGame {
                start: Instant::now(),
                prev_v: u64::MAX,
            }));
        }
        None
    }
}

/// A simple "game" that counts from 0 to 5, then ends.
struct TimerGame {
    start: Instant,
    prev_v: u64,
}

#[async_trait]
impl GameState<GlobalState> for TimerGame {
    async fn update(&mut self, lobby: &mut Lobby<GlobalState>) -> bool {
        // how many seconds have elapsed?
        let v = self.start.elapsed().as_secs();
        // do we have to update the clients?
        if v != self.prev_v {
            self.prev_v = v;
            // more than 5 seconds -> the game ends
            if v > 5 {
                return true;
            }
            // update clients
            for player in lobby.players_mut() {
                player.send(format!("=<h1>T: {v}</h1>")).await;
            }
        }
        for player in lobby.players_mut() {
            if let Some(_) = player.get_msg().await {
                // ...
            }
        }
        false
    }
    async fn player_leaving(&mut self, _lobby: &mut Lobby<GlobalState>, _player: PlayerIndex) {}
}
