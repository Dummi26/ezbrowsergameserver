use std::net::SocketAddr;

use clap::Parser;

mod ezgame;
mod site;

#[derive(Parser)]
struct Args {
    site_addr: SocketAddr,
    socket_addr: SocketAddr,
}

// Server -> Client
// P -> Player List (as html)
// L<txt> set left text
// R<txt> set right text
// 1<t> start game: set countdown
// 2 start game/next round
// 3<html> round end
// 0<id> back to lobby (set ready to false)
// Client -> Server
// -<myname> set my name (lobby)
// 0 not ready (lobby)
// 1 ready (lobby)
// L<vote> (ingame)
// R<vote> (ingame)
// =<mytxt> set my text (ingame)
// Settings (Client <-> Server | lobby)
// sMP<max_points>

#[tokio::main]
async fn main() {
    let args = Args::parse();
    _ = tokio::spawn(ezgame::main(args.socket_addr.clone()));
    let site = site::Site::new(args.site_addr, args.socket_addr);
    site.main().await;
}
