use std::task::Poll;

use async_trait::async_trait;
use futures_util::{SinkExt, TryStreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

pub struct Lobby<S: LobbyState> {
    pub state: S,
    pub(crate) players: Vec<PlayerCon<S::PlayerState>>,
    /// set to true
    /// - manually
    /// - in new lobby
    /// - after playing a game
    /// indicates that some state should be reset (player ready status, ...)
    pub reset: bool,
}

/// LobbyState is the state stored in every lobby.
///
/// When a new lobby is created, `new()` is called to generate one instance of this struct.
///
/// When a new player joins a lobby, `new_player()` is called to generate an instance of `Self::PlayerState`.
/// Then, player_joined() is called.
///
/// When a player disconnects (this can only be detected after a call to `get_msg()` or `send()`),
/// `player_leaving()` is called. After `player_leaving()`, the player is removed from `lobby.players`.
///
/// `lobby_update()` is called repeatedly until it returns Some(_), which starts a game.
/// Once the game finishes, the lobby's update loop restarts and `lobby.reset` is set to true.
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
    /// Called when a player disconnects, before they are removed
    async fn player_leaving(id: usize, lobby: &mut Lobby<Self>, player: PlayerIndex);
    /// Called repeatedly while in the lobby phase.
    /// Return Some(_) to start a game.
    /// Since you're returning a trait object (`dyn GameState`),
    /// you could create multiple structs implementing `GameState`
    /// and returning either one, possibly at random
    /// or based on `lobby.state` (user chooses a gamemode).
    async fn lobby_update(id: usize, lobby: &mut Lobby<Self>) -> Option<Box<dyn GameState<Self>>>;
}

/// GameState is the state used during a game.
///
/// During a game, `update()` is called repeatedly.
/// Once `update()` returns `true`, the game ends.
/// You can send/receive messages from the clients using `lobby`.
#[async_trait]
pub trait GameState<S: LobbyState>: Send {
    // update your game.
    // return true to end the game and return to the lobby.
    async fn update(&mut self, lobby: &mut Lobby<S>) -> bool;
    async fn player_leaving(&mut self, lobby: &mut Lobby<S>, player: PlayerIndex);
}

pub(crate) struct InGame<S: LobbyState> {
    pub(crate) lobby: Lobby<S>,
    pub(crate) game_state: Box<dyn GameState<S>>,
}

pub struct PlayerCon<D> {
    pub data: D,
    con: Option<WebSocketStream<TcpStream>>,
}

/// Index of a player that exists.
/// Allows you to use `lobby.get_player()` without dealing with the index-out-of-bounds cases,
/// since this index is never out-of-bounds.
#[derive(Clone, Copy)]
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
    pub(crate) async fn update(&mut self) -> bool {
        self.game_state.update(&mut self.lobby).await
    }
    pub(crate) async fn player_leaving(&mut self, index: usize) {
        self.game_state
            .player_leaving(&mut self.lobby, PlayerIndex(index))
            .await;
    }
}

impl<D> PlayerCon<D> {
    pub(crate) fn new(data: D, con: WebSocketStream<TcpStream>) -> Self {
        Self {
            data,
            con: Some(con),
        }
    }
    /// forcibly disconnects this player.
    pub async fn force_disconnect(&mut self) {
        if let Some(con) = &mut self.con {
            _ = con.close(None).await;
            self.con = None;
        }
    }
    pub fn disconnected(&self) -> bool {
        self.con.is_none()
    }
    pub async fn send(&mut self, msg: String) {
        if let Some(con) = &mut self.con {
            if con.send(Message::Text(msg)).await.is_err() {
                self.con = None;
            }
        }
    }
    // like `get_msg`, but blocking
    pub async fn wait_for_msg(&mut self) -> Option<String> {
        if let Some(con) = &mut self.con {
            if let Ok(Some(msg)) = con.try_next().await {
                self.respond_msg(msg).await
            } else {
                None
            }
        } else {
            None
        }
    }
    pub async fn get_msg(&mut self) -> Option<String> {
        if let Some(con) = &mut self.con {
            if let Poll::Ready(Ok(Some(msg))) = futures_util::poll!(con.try_next()) {
                self.respond_msg(msg).await
            } else {
                None
            }
        } else {
            None
        }
    }
    async fn respond_msg(&mut self, msg: Message) -> Option<String> {
        match msg {
            Message::Text(msg) => Some(msg),
            Message::Close(_) => {
                self.force_disconnect().await;
                None
            }
            Message::Binary(_) => None,
            Message::Ping(data) => {
                if let Some(con) = &mut self.con {
                    _ = con.send(Message::Pong(data));
                }
                None
            }
            Message::Pong(_) => None,
            Message::Frame(_) => None,
        }
    }
}
