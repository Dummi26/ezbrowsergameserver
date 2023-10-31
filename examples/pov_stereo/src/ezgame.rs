use std::time::{Duration, Instant};

use ezbrowsergameserver::prelude::*;
use rand::seq::SliceRandom;
use tokio::net::ToSocketAddrs;

pub async fn main(addr: impl ToSocketAddrs + Send + 'static) {
    host::<LobbyS>(addr).await;
}

struct LobbyS {
    ready_since: Option<(Instant, u64)>,
    max_points: isize,
}

impl LobbyS {
    async fn update_players_list(lobby: &mut Lobby<Self>) {
        let msg = ["P".to_owned()]
            .into_iter()
            .chain(lobby.players().iter().map(|p| {
                if p.data.ready {
                    format!("<div><b>{}</b></div>", p.data.name)
                } else {
                    format!("<div>{}</div>", p.data.name)
                }
            }))
            .collect::<String>();
        for player in lobby.players_mut() {
            player.send(msg.clone()).await;
        }
    }
}

struct PlayerS {
    name: String,
    text: String,
    ready: bool,
    points_this_round: isize,
    points_total: isize,
    /// i8::MIN -> no vote yet
    received_vote: (i8, i8),
    leftright: Option<(PlayerIndex, PlayerIndex)>,
}

#[async_trait]
impl LobbyState for LobbyS {
    type PlayerState = PlayerS;
    fn new() -> Self {
        Self {
            ready_since: None,
            max_points: 30,
        }
    }
    fn new_player() -> Self::PlayerState {
        Self::PlayerState {
            name: String::new(),
            text: String::new(),
            ready: false,
            points_this_round: 0,
            points_total: 0,
            received_vote: (i8::MIN, i8::MIN),
            leftright: None,
        }
    }
    async fn player_joined(id: usize, lobby: &mut Lobby<Self>, player: PlayerIndex) {
        Self::update_players_list(lobby).await;
        let mp = lobby.state.max_points;
        let p = lobby.get_player(player);
        p.send(format!("0{id}")).await;
        p.send(format!("sMP{mp}")).await;
    }
    async fn player_leaving(_id: usize, lobby: &mut Lobby<Self>, _player: PlayerIndex) {
        Self::update_players_list(lobby).await;
    }
    async fn lobby_update(id: usize, lobby: &mut Lobby<Self>) -> Option<Box<dyn GameState<Self>>> {
        let mut update_list = false;
        if lobby.reset {
            lobby.reset = false;
            update_list = true;
            for player in lobby.players_mut() {
                player.data.ready = false;
                player.send(format!("0{id}")).await;
            }
        }
        // messages
        for player_index in lobby.player_indices() {
            let player = lobby.get_player(player_index);
            if let Some(msg) = player.get_msg().await {
                match msg.chars().next() {
                    Some('0') => {
                        player.data.ready = false;
                        update_list = true;
                    }
                    Some('1') => {
                        player.data.ready = true;
                        update_list = true;
                    }
                    Some('-') => {
                        player.data.name = msg[1..].to_owned();
                        update_list = true;
                    }
                    Some('s') => match msg.chars().skip(1).take(2).collect::<String>().as_str() {
                        "MP" => {
                            if let Ok(v) = msg[3..].parse() {
                                lobby.state.max_points = v;
                                for (i, player) in lobby.players_mut().enumerate() {
                                    if i != player_index.i() {
                                        player.send(format!("sMP{v}")).await;
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        if update_list {
            Self::update_players_list(lobby).await;
        }
        // game start (countdown)
        if lobby
            .players()
            .iter()
            .all(|p| p.data.ready && !p.data.name.trim().is_empty())
        {
            if lobby.state.ready_since.is_none() {
                lobby.state.ready_since = Some((Instant::now(), u64::MAX));
            }
        } else {
            if lobby.state.ready_since.is_some() {
                for player in lobby.players_mut() {
                    player.send(format!("0{id}")).await;
                }
                lobby.state.ready_since = None;
            }
        }
        if let Some((since, prev)) = &mut lobby.state.ready_since {
            let secs = since.elapsed().as_secs();
            if secs != *prev {
                *prev = secs;
                if secs >= 3 {
                    return Some(Box::new(GameS {
                        force_exit: false,
                        round: 0,
                        players: lobby.player_indices().collect(),
                    }));
                } else {
                    for player in lobby.players_mut() {
                        player.send(format!("1{}", 3 - secs)).await;
                    }
                }
            }
        }
        None
    }
}

struct GameS {
    force_exit: bool,
    round: usize,
    players: Vec<PlayerIndex>,
}

#[async_trait]
impl GameState<LobbyS> for GameS {
    async fn update(&mut self, lobby: &mut Lobby<LobbyS>) -> bool {
        if self.force_exit
            || self.round == 0
            || lobby
                .players()
                .iter()
                .all(|p| p.data.received_vote.0 != i8::MIN && p.data.received_vote.1 != i8::MIN)
        {
            if self.force_exit || self.round > 0 {
                if !self.force_exit {
                    // don't do this when force_exiting because some received_votes may be invalid (i8::MIN)
                    for player in lobby.players_mut() {
                        player.data.points_this_round =
                            (player.data.received_vote.0 + player.data.received_vote.1) as _;
                        player.data.points_total += player.data.points_this_round;
                    }
                }
                let mut players = lobby.players().iter().map(|p| &p.data).collect::<Vec<_>>();
                players.sort_by_key(|p| p.points_total);
                let (best, best2) = if !self.force_exit {
                    // find this rounds top player. if tie, player with least total points wins.
                    let best = players.iter().max_by_key(|p| p.points_this_round).unwrap();
                    (vec![format!(
                        "<div style=\"position:fixed;top:0;left:0;height:48%;width:50%;\">{}</div><div style=\"position:fixed;top:50%\">",
                        best.text
                    )], vec![format!("</div>")])
                } else {
                    (vec![], vec![])
                };
                let msg = ["3".to_owned()]
                    .into_iter()
                    .chain(best)
                    .chain(players.into_iter().rev().map(|p| {
                        format!(
                            "<p>{} <small>({:+})</small>: <b>{}</b></p>",
                            p.name, p.points_total, p.points_this_round
                        )
                    }))
                    .chain(best2)
                    .collect::<String>();
                for player in lobby.players_mut() {
                    player.send(msg.clone()).await;
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
                if self.force_exit
                    || lobby
                        .players()
                        .iter()
                        .map(|p| p.data.points_total)
                        .max()
                        .is_some_and(|v| v >= lobby.state.max_points)
                {
                    return true;
                }
            }
            self.round += 1;
            for player in lobby.players_mut() {
                player.data.received_vote = (i8::MIN, i8::MIN);
                player.data.text = String::new();
                player.send(format!("2")).await;
            }
            self.players.shuffle(&mut rand::thread_rng());
            for (i, player) in self.players.iter().enumerate() {
                let l = if i == 0 {
                    self.players[self.players.len() - 1]
                } else {
                    self.players[i - 1]
                };
                let r = if i == self.players.len() - 1 {
                    self.players[0]
                } else {
                    self.players[i + 1]
                };
                lobby.get_player(*player).data.leftright = Some((l, r));
            }
        }
        // messages
        for player_index in lobby.player_indices() {
            let player = lobby.get_player(player_index);
            if let Some(msg) = player.get_msg().await {
                match msg.chars().next() {
                    Some('L') => {
                        if let Ok(vote) = msg[1..].parse() {
                            let p = player.data.leftright.unwrap().0;
                            lobby.get_player(p).data.received_vote.1 = vote;
                        }
                    }
                    Some('R') => {
                        if let Ok(vote) = msg[1..].parse() {
                            let p = player.data.leftright.unwrap().1;
                            lobby.get_player(p).data.received_vote.0 = vote;
                        }
                    }
                    Some('=') => {
                        let text = msg[1..].to_owned();
                        let lr = player.data.leftright.unwrap();
                        lobby.get_player(lr.0).send(format!("R{text}")).await;
                        lobby.get_player(lr.1).send(format!("L{text}")).await;
                        lobby.get_player(player_index).data.text = text;
                    }
                    _ => {}
                }
            }
        }
        false
    }
    async fn player_leaving(&mut self, _lobby: &mut Lobby<LobbyS>, _player: PlayerIndex) {
        self.force_exit = true;
    }
}
