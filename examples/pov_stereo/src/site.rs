use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    response::Html,
    routing::get,
    Router,
};

type S = Arc<Site>;
pub struct Site {
    site_addr: SocketAddr,
    html: String,
}
impl Site {
    pub fn new(site_addr: SocketAddr, socket_addr: SocketAddr) -> Self {
        let port = socket_addr.port();
        Self {
            site_addr,
            html: include_str!("site.html").replace("\\{port}", port.to_string().as_str()),
        }
    }
    pub async fn main(self) -> ! {
        let addr = self.site_addr.clone();
        let r = Router::new()
            .route("/", get(Self::root))
            .route("/join/:id", get(Self::join))
            .with_state(Arc::new(self));
        axum::Server::bind(&addr)
            .serve(r.into_make_service())
            .await
            .unwrap();
        panic!();
    }
    async fn root(State(s): State<S>) -> Html<String> {
        Html(s.html.replace("\\{autojoin}", ""))
    }
    async fn join(State(s): State<S>, Path(id): Path<String>) -> Html<String> {
        Html(s.html.replace(
            "\\{autojoin}",
            format!("connectToLobby(\"{id}\");").as_str(),
        ))
    }
}
