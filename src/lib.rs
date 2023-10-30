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
/// to specify your `LobbyState` type, use the `host::<YourType>(addr).await` syntax.
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
                    for index in lobby
                        .players()
                        .iter()
                        .enumerate()
                        .filter_map(|(i, p)| if p.disconnected() { Some(i) } else { None })
                        .collect::<Vec<_>>()
                    {
                        S::player_leaving(i, lobby, PlayerIndex(index)).await;
                        lobby.players.remove(index);
                    }
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
        let indices = in_game
            .lobby
            .players()
            .iter()
            .enumerate()
            .filter_map(|(i, p)| if p.disconnected() { Some(i) } else { None })
            .collect::<Vec<_>>();
        for index in indices.into_iter().rev() {
            in_game.player_leaving(index).await;
            in_game.lobby.players.remove(index);
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
