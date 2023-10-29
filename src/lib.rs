use std::{sync::Arc, time::Duration};

use futures_util::TryStreamExt;
use game::{LobbyState, PlayerCon, PlayerIndex};
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::Mutex,
};

use crate::game::{InGame, Lobby};

pub mod game;

pub mod prelude {
    pub use crate::{
        game::{GameState, Lobby, LobbyState, PlayerIndex},
        host,
    };
    pub use async_trait::async_trait;
}

/// Hosts the game's WebSocket.
///
/// this function ends in an infinite loop, so it never returns.
/// if you would like to periodically do something, use `in_loop` for that.
///
/// clients can connect to the WebSocket via `addr`.
/// they must then send two messages:
/// - their nickname
/// - either "new" or an id indicating which lobby they want to join
///
/// once in a lobby, message handling is done via `lobby_update`.
/// this should
/// - give clients a way to ask for initial info (other players, settings, lobby id, ...)
/// - give clients a way to change their name (should notify all other clients)
/// - let some/all clients change the game settings
/// - do whatever else you want to be able to do in the lobby
/// within `lobby_update`, you can return `Some(_)` to start the game.
/// which `GameState` you return here can depend on the settings (i.e. your `Settings` may be an enum so users can pick one of multiple games), it just has to be any value implementing the `GameState` trait.
///
/// while a game is running, the `GameState` handles all messages.
/// see the `GameState` trait for more info.
pub async fn host<S: LobbyState + 'static>(addr: impl ToSocketAddrs + Send + 'static) -> ! {
    let lobbies: Arc<Mutex<Vec<Option<Lobby<S>>>>> = Default::default();
    tokio::spawn(accept_new(addr, Arc::clone(&lobbies)));
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut lock = lobbies.lock().await;
        for (i, l) in lock.iter_mut().enumerate() {
            if let Some(lobby) = l {
                if lobby.players().is_empty() {
                    *l = None;
                } else {
                    if let Some(game_state) = S::lobby_update(i, lobby).await {
                        let ig = InGame::new(l.take().unwrap(), game_state);
                        tokio::spawn(in_game(ig, Arc::clone(&lobbies)));
                    }
                }
            }
        }
    }
}

async fn in_game<S: LobbyState + 'static>(
    mut in_game: InGame<S>,
    lobbies: Arc<Mutex<Vec<Option<Lobby<S>>>>>,
) {
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if in_game.update().await {
            add_lobby(lobbies.lock().await.as_mut(), in_game.into_lobby());
            return;
        }
    }
}

async fn accept_new<S: LobbyState + 'static>(
    addr: impl ToSocketAddrs + Send,
    lobbies: Arc<Mutex<Vec<Option<Lobby<S>>>>>,
) {
    let server = TcpListener::bind(addr).await.unwrap();
    loop {
        match server.accept().await {
            Ok((con, _)) => {
                tokio::spawn(handle_new_connection(con, Arc::clone(&lobbies)));
            }
            _ => {}
        }
    }
}

fn add_lobby<S: LobbyState>(lobbies: &mut Vec<Option<Lobby<S>>>, lobby: Lobby<S>) -> usize {
    if let Some(i) = lobbies.iter().position(|l| l.is_none()) {
        lobbies[i] = Some(lobby);
        i
    } else {
        lobbies.push(Some(lobby));
        lobbies.len() - 1
    }
}

async fn handle_new_connection<S: LobbyState>(
    con: TcpStream,
    lobbies: Arc<Mutex<Vec<Option<Lobby<S>>>>>,
) {
    if let Ok(mut con) = tokio_tungstenite::accept_async(con).await {
        if let Ok(Some(msg)) = con.try_next().await {
            if let Ok(lobby) = msg.into_text() {
                let player = PlayerCon::new(S::new_player(), con);
                if lobby == "new" {
                    let l = Lobby::new(S::new(), vec![player]);
                    let mut lobbies_lock = lobbies.lock().await;
                    let lobby = add_lobby(lobbies_lock.as_mut(), l);
                    S::player_joined(lobby, lobbies_lock[lobby].as_mut().unwrap(), PlayerIndex(0))
                        .await;
                } else if let Ok(lobby) = usize::from_str_radix(&lobby, 16) {
                    let mut lobbies_lock = lobbies.lock().await;
                    if let Some(Some(l)) = lobbies_lock.get_mut(lobby) {
                        let pindex = l.players().len();
                        l.join(player);
                        S::player_joined(lobby, l, PlayerIndex(pindex)).await;
                    }
                }
            }
        }
        // let mut con = PlayerCon::new(String::new(), con);
    }
}
