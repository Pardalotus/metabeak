use axum::{
    extract::{Multipart, State},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};

use crate::{db, execution::model::HandlerSpec, service, util::VERSION};

async fn heartbeat(State(shared_state): State<Pool<Postgres>>) -> Response {
    match db::pool::heartbeat(&shared_state).await {
        Ok(result) if result => (
            StatusCode::OK,
             axum::Json(
                serde_json::json!({"heartbeat": result, "platform": "Pardalotus API", "version": VERSION}),
            ),
        ),
        Err(e) => {
            log::error!("Heartbeat failure: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"heartbeat": false, "platform": "Pardalotus API", "version": VERSION})),
            )
        }
        _ => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"heartbeat": false, "platform": "Pardalotus API", "version": VERSION})),
            )
        }
    }.into_response()
}

#[derive(Serialize)]
struct HandlerPage {
    results: Vec<HandlerSpec>,
}

async fn list_functions(State(shared_state): State<Pool<Postgres>>) -> Response {
    match service::list_handlers(&shared_state).await {
        Ok(result) => (StatusCode::OK, Json(HandlerPage { results: result })).into_response(),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "can't fetch handlers"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct HandlerPost {
    name: String,
    email: String,
}

async fn post_function(State(pool): State<Pool<Postgres>>, mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "data" {
            if let Ok(data) = field.text().await {
                let task = HandlerSpec {
                    handler_id: -1,
                    code: data,
                };

                return match service::load_handler(&pool, &task).await {
                    service::TaskLoadResult::Exists { task_id } => (
                        StatusCode::OK,
                        Json(serde_json::json!({"status": "already-exists", "task_id": task_id})),
                    )
                        .into_response(),
                    service::TaskLoadResult::New { task_id } => (
                        StatusCode::CREATED,
                        Json(serde_json::json!({"status": "created", "task-id": task_id})),
                    )
                        .into_response(),
                    service::TaskLoadResult::FailedSave() => (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"status": "failed"})),
                    )
                        .into_response(),
                };
            }
        }
    }

    return (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"status": "failed", "reason": "No function supplied. Please check the documentation."})),
    )
        .into_response();
}

pub(crate) async fn run(pool: &Pool<Postgres>) {
    let app = Router::new()
        .route("/functions/", get(list_functions).post(post_function))
        .route("/heartbeat", get(heartbeat))
        .with_state(pool.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:6464").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
