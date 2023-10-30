use std::time::Instant;

use ezbrowsergameserver::prelude::*;
use rand::{prelude::SliceRandom, Rng};

#[tokio::main]
async fn main() {
    host::<GlobalState>("0.0.0.0:8081").await;
}

struct GlobalState {
    update: bool,
}
struct PS {
    name: String,
    ready: bool,
    time: f32,
    text: String,
}

#[async_trait]
impl LobbyState for GlobalState {
    type PlayerState = PS;
    fn new() -> Self {
        Self { update: true }
    }
    fn new_player() -> Self::PlayerState {
        PS {
            name: String::new(),
            ready: false,
            time: 0.0,
            text: String::new(),
        }
    }
    async fn player_joined(_id: usize, lobby: &mut Lobby<Self>, _player: PlayerIndex) {
        lobby.state.update = true;
    }
    async fn player_leaving(_id: usize, lobby: &mut Lobby<Self>, _player: PlayerIndex) {
        lobby.state.update = true;
    }
    async fn lobby_update(id: usize, lobby: &mut Lobby<Self>) -> Option<Box<dyn GameState<Self>>> {
        let mut update = false;
        if lobby.state.update {
            update = true;
            lobby.state.update = false;
        }
        if lobby.reset {
            lobby.reset = false;
            update = true;
            // show lobby screen to all clients
            for player in lobby.players_mut() {
                player.data.ready = false;
                player.send(format!("1{id:X}")).await;
            }
        }
        for player in lobby.players_mut() {
            if let Some(msg) = player.get_msg().await {
                match msg.chars().next() {
                    Some('n') => {
                        player.data.name = msg[1..].to_owned();
                        update = true;
                    }
                    Some('R') => {
                        player.data.ready = true;
                        update = true;
                    }
                    Some('r') => {
                        player.data.ready = false;
                        update = true;
                    }
                    _ => (),
                }
            }
        }
        if update {
            for player in lobby.player_indices() {
                let html = lobby
                    .players()
                    .iter()
                    .map(|p| {
                        let name = if p.data.name.trim().is_empty() {
                            "[new player]"
                        } else {
                            p.data.name.trim()
                        };
                        if p.data.ready {
                            format!("<p><b>{}</b></p>", name)
                        } else {
                            format!("<p>{}</p>", name)
                        }
                    })
                    .collect::<String>();
                let player = lobby.get_player(player);
                player.send(format!("p{html}")).await;
            }
            if lobby
                .players()
                .iter()
                .all(|p| p.data.ready && !p.data.name.is_empty())
            {
                return Some(Box::new(TypingGame::new()));
            }
        }
        None
    }
}

struct TypingGame {
    target: String,
    state: u8,
    start: Instant,
    last_start: u64,
}
impl TypingGame {
    pub fn new() -> TypingGame {
        Self {
            target: String::new(),
            state: 0,
            start: Instant::now(),
            last_start: u64::MAX,
        }
    }
}

#[async_trait]
impl GameState<GlobalState> for TypingGame {
    async fn update(&mut self, lobby: &mut Lobby<GlobalState>) -> bool {
        // new target, update all clients
        let mut update_game_list = false;
        if self.state == 0 {
            let secs = self.start.elapsed().as_secs();
            if secs != self.last_start {
                self.last_start = secs;
                let countdown = 3u64.saturating_sub(secs);
                let target = get_phrase();
                for player in lobby.players_mut() {
                    if countdown == 0 {
                        player.data.text = String::new();
                        player.data.time = -1.0;
                        player.send(format!("3{target}")).await;
                    } else {
                        player.send(format!("2{countdown}...")).await;
                    }
                }
                if countdown == 0 {
                    self.target = target;
                    self.start = Instant::now();
                    self.state = 1;
                    update_game_list = true;
                }
            }
        }
        if self.state == 1 {
            // messages
            for player in lobby.players_mut() {
                if let Some(msg) = player.get_msg().await {
                    if msg.starts_with("=") {
                        update_game_list = true;
                        player.data.text = msg[1..].to_owned();
                        if player.data.text == self.target {
                            let secs = self.start.elapsed().as_secs_f32();
                            player.data.time = secs;
                            player.send(format!("X{:.2}", secs)).await;
                        }
                    }
                }
            }
        }
        if update_game_list {
            let max_time = lobby
                .players()
                .iter()
                .filter(|v| v.data.time >= 0.0)
                .map(|v| v.data.time)
                .fold(1.0, f32::max);
            let mut list = lobby
                .players()
                .iter()
                .map(|p| {
                    let mut target_chars = self.target.chars();
                    let text = p
                        .data
                        .text
                        .chars()
                        .take_while(|c| target_chars.next().is_some_and(|t| t == *c))
                        .collect::<String>();
                    let rest = &p.data.text[text.len()..];
                    (
                        format!(
                            "<p><b>{text}</b>{rest} &emsp; <small>[{}{}]</small></p>",
                            p.data.name,
                            if p.data.time >= 0.0 {
                                format!(" - {:.2}s", p.data.time)
                            } else {
                                String::new()
                            }
                        ),
                        if p.data.time >= 0.0 {
                            -(self.target.len() as f32) - max_time + p.data.time
                        } else {
                            -(text.len() as f32)
                        },
                    )
                })
                .collect::<Vec<_>>();
            list.sort_by(|a, b| a.1.total_cmp(&b.1));
            for player in lobby.players_mut() {
                player
                    .send(format!(
                        "G{}",
                        list.iter().map(|v| v.0.as_str()).collect::<String>()
                    ))
                    .await;
            }
        }
        // game ends once all players finish
        if self.state == 1 && lobby.players().iter().all(|p| p.data.time >= 0.0) {
            self.start = Instant::now();
            self.state = 2;
        }
        // exit after 3 seconds
        self.state == 2 && self.start.elapsed().as_secs() >= 3
    }
    async fn player_leaving(&mut self, _lobby: &mut Lobby<GlobalState>, _player: PlayerIndex) {}
}

fn get_phrase() -> String {
    let words = include_str!("words.txt").lines().collect::<Vec<_>>();
    let mut out = String::new();
    let mut rng = rand::thread_rng();
    for i in 0..rng.gen_range(5..=20) {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(words.choose(&mut rng).unwrap());
    }
    out
}
