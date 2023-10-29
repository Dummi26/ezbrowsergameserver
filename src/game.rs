use std::task::Poll;

use async_trait::async_trait;
use futures_util::{SinkExt, TryStreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

pub struct Lobby<S: LobbyState> {
    pub state: S,
    players: Vec<PlayerCon<S::PlayerState>>,
    /// set to true
    /// - manually
    /// - in new lobby
    /// - after playing a game
    /// indicates that some state should be reset (player ready status, ...)
    pub reset: bool,
}

#[async_trait]
pub trait LobbyState: Send + Sized {
    /// State associated with each player,
    /// accessible via `player.data`.
    type PlayerState: Send;
    /// This creates the default state/settings for a new lobby
    fn new() -> Self;
    /// This creates the default state for a newly joined player
    fn new_player() -> Self::PlayerState;
    /// Called when a new player joins the lobby
    async fn player_joined(id: usize, lobby: &mut Lobby<Self>, player: PlayerIndex);
    /// Called repeatedly while in the lobby phase.
    /// Return Some(_) to start a game.
    /// Note: GameState's actual type can depend on the values in `lobby.state` (type `Self`), for example.
    async fn lobby_update(id: usize, lobby: &mut Lobby<Self>) -> Option<Box<dyn GameState<Self>>>;
}

#[async_trait]
pub trait GameState<S: LobbyState>: Send {
    // update your game.
    // return true to end the game and return to the lobby.
    async fn update(&mut self, lobby: &mut Lobby<S>) -> bool;
}

pub struct InGame<S: LobbyState> {
    lobby: Lobby<S>,
    game_state: Box<dyn GameState<S>>,
}

pub struct PlayerCon<D> {
    pub data: D,
    con: Option<WebSocketStream<TcpStream>>,
}

/// index of a player that exists
pub struct PlayerIndex(pub(crate) usize);
impl PlayerIndex {
    pub fn i(&self) -> usize {
        self.0
    }
}

impl<S: LobbyState> Lobby<S> {
    pub(crate) fn new(settings: S, players: Vec<PlayerCon<S::PlayerState>>) -> Self {
        Self {
            state: settings,
            players,
            reset: true,
        }
    }
    pub(crate) fn join(&mut self, player: PlayerCon<S::PlayerState>) {
        self.players.push(player);
    }
    pub fn settings(&self) -> &S {
        &self.state
    }
    pub fn settings_mut(&mut self) -> &mut S {
        &mut self.state
    }
    pub fn get_player(&mut self, player: PlayerIndex) -> &mut PlayerCon<S::PlayerState> {
        &mut self.players[player.0]
    }
    pub fn players(&self) -> &Vec<PlayerCon<S::PlayerState>> {
        &self.players
    }
    pub fn players_mut(&mut self) -> std::slice::IterMut<PlayerCon<S::PlayerState>> {
        self.players.iter_mut()
    }
    pub fn player_indices(&self) -> impl Iterator<Item = PlayerIndex> {
        (0..self.players.len()).map(|v| PlayerIndex(v))
    }
}

impl<S: LobbyState> InGame<S> {
    pub(crate) fn new(lobby: Lobby<S>, game_state: Box<dyn GameState<S>>) -> Self {
        Self { lobby, game_state }
    }
    pub(crate) fn into_lobby(mut self) -> Lobby<S> {
        self.lobby.reset = true;
        self.lobby
    }
    pub fn lobby(&self) -> &Lobby<S> {
        &self.lobby
    }

    pub(crate) async fn update(&mut self) -> bool {
        self.game_state.update(&mut self.lobby).await
    }
}

impl<D> PlayerCon<D> {
    pub fn new(data: D, con: WebSocketStream<TcpStream>) -> Self {
        Self {
            data,
            con: Some(con),
        }
    }
    pub async fn send(&mut self, msg: String) {
        if let Some(con) = &mut self.con {
            if con.send(Message::Text(msg)).await.is_err() {
                self.con = None;
            }
        }
    }
    pub async fn get_msg(&mut self) -> Option<String> {
        if let Some(con) = &mut self.con {
            if let Poll::Ready(Ok(Some(msg))) = futures_util::poll!(con.try_next()) {
                match msg {
                    Message::Text(msg) => Some(msg),
                    Message::Close(_) => {
                        _ = con.close(None).await;
                        self.con = None;
                        None
                    }
                    Message::Binary(_) => None,
                    Message::Ping(data) => {
                        _ = con.send(Message::Pong(data));
                        None
                    }
                    Message::Pong(_) => None,
                    Message::Frame(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}
