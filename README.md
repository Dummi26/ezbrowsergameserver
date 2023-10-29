# ezbrowsergameserver

Ever wanted to make and self-host one of those simple, usually text-based, multiplayer browser games?

Me too! And this is a small library to hopefully make that as ez as possible :)

## Getting started

Before you do anything, know that `ezbrowsergameserver` isn't
a server for your website - it only provides the WebSocket
which will be used to keep all your players updated and in sync.

If you have python3 installed, you can use the script `examples/server.sh` as a quick
no-setup web server running on `0.0.0.0:8080`.

For this example, you will use `0.0.0.0:8080/00_min.html`:

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="UTF-8">
    <title>Min - ezbrowsergameserver</title>
    <meta name="color-scheme" content="light dark">
    <script>
      var con = undefined;
      function createLobby() {
        joinLobby("new");
      }
      function joinLobby(id) {
        let ip;
        // if possible, use the same host that is hosting this html file.
        // assume localhost if not possible (i.e. opened the file directly)
        if (window.location.hostname) {
          ip = window.location.hostname;
        } else {
          ip = "0.0.0.0";
        }
        // connect to the websocket
        con = new WebSocket("ws://" + ip + ":8081");
        // handle incoming messages
        con.onmessage = (e) => {
          let msg = e.data;
          // allow the server to update the clients html
          bodyDiv.innerHTML = msg;
        };
        // when the websocket finishes connecting...
        con.onopen = () => {
          // join a lobby
          con.send(id);
        };
      }
    </script>
  </head>
  <body>
    <div id="bodyDiv">
      <p>Lobby ID: <input id="lobbyId"></p>
      <button onclick=createLobby()>Create new lobby</button>
      <button onclick=joinLobbyPressed()>Join lobby by ID</button>
      <script>
        function joinLobbyPressed() {
          joinLobby(lobbyId.value);
        }
      </script>
    </div>
  </body>
</html>
```

This will let you create a new lobby or join an existing one by entering its ID in the text input.
The `<script>` sections use a WebSocket to connect to your game server - the server
you are about to create using this library.

But before we get to your very first ezbrowsergame, you need a lobby.

The lobby is the place where people gather before a game starts.
While in the lobby, people can change the gamemode, the game's settings,
or anything else you implement for your lobby.
The lobby is also the place for your global state, since it still exists when a game ends.
Lobbies can be accessed via their ID, formatted as a hex string (`{id:X}`).

This example will simply start the "game" every time a player joins the lobby.
The "game" will last one second and show the new number of players in the lobby.

These are the imports you will need for the example:

```rust
use std::time::Instant;
use ezbrowsergameserver::prelude::*;
```

First, create a struct for your global state:

```rust
struct GlobalState {
    joined: bool,
}
```

Then, one for your game:

```rust
struct PlayerCountGame(Option<Instant>);
```

Now, implement `LobbyState` for your `GlobalState` struct:

```rust
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
```

Then, implement `GameState` for your `PlayerCountGame`:

```rust
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
```

And finally, add your `main` function:

```rust
#[tokio::main]
async fn main() {
    host::<GlobalState>("0.0.0.0:8081").await;
}
```

That's it - we've created a "game".

If you want to try it out, go to the `examples/` directory,
run `./server.sh`, run `cargo run --example 00_min`,
then open `0.0.0.0:8080/00_min.html` in a browser and create a new lobby.
