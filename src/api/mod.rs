use axum::{
    extract::{self, State},
    routing::get,
    Router,
};
use serde_json::Value;
use sqlx::{Pool, Postgres};

use crate::{db, util::VERSION};

async fn heartbeat(State(shared_state): State<Pool<Postgres>>) -> extract::Json<Value> {
    match db::pool::heartbeat(&shared_state).await {
        Ok(result) => extract::Json::from(
            serde_json::json!({"heartbeat": result, "platform": "Pardalotus API", "version": VERSION}),
        ),
        Err(e) => {
            log::error!("Heartbeat failure: {:?}", e);
            extract::Json::from(
                serde_json::json!({"heartbeat": false, "platform": "Pardalotus API", "version": VERSION}),
            )
        }
    }
}

pub(crate) async fn run(pool: &Pool<Postgres>) {
    let app = Router::new()
        .route("/heartbeat", get(heartbeat))
        .with_state(pool.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:6464").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
