use std::time::Instant;

use ezbrowsergameserver::prelude::*;

#[tokio::main]
async fn main() {
    host::<GlobalState>("0.0.0.0:8081").await;
}

struct GlobalState {
    joined: bool,
}

#[async_trait]
impl LobbyState for GlobalState {
    type PlayerState = ();
    fn new() -> Self {
        Self { joined: false }
    }
    fn new_player() -> Self::PlayerState {
        ()
    }
    async fn player_joined(_id: usize, lobby: &mut Lobby<Self>, _player: PlayerIndex) {
        lobby.state.joined = true;
    }
    async fn lobby_update(id: usize, lobby: &mut Lobby<Self>) -> Option<Box<dyn GameState<Self>>> {
        if lobby.reset {
            lobby.reset = false;
            // show lobby screen to all clients
            for (i, player) in lobby.players_mut().enumerate() {
                player
                    .send(format!(
                        "<h1>You are player #{}</h1><p>Lobby ID: {id:X}</p>",
                        i + 1
                    ))
                    .await;
            }
        }
        if lobby.state.joined {
            lobby.state.joined = false;
            // start the "game"
            return Some(Box::new(PlayerCountGame(None)));
        }
        None
    }
}

struct PlayerCountGame(Option<Instant>);

#[async_trait]
impl GameState<GlobalState> for PlayerCountGame {
    async fn update(&mut self, lobby: &mut Lobby<GlobalState>) -> bool {
        // game starts, update all clients
        if self.0.is_none() {
            self.0 = Some(Instant::now());
            let c = lobby.players().len();
            for player in lobby.players_mut() {
                player.send(format!("<h1>There are {c} players</h1>")).await;
            }
        }
        // game ends after 1 seconds
        self.0.is_some_and(|start| start.elapsed().as_secs() >= 1)
    }
}
