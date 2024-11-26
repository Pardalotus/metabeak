use axum::{
    extract::{Multipart, Path, Query, State},
    http::HeaderValue,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use axum_extra::response::ErasedJson;
use reqwest::{header::CONTENT_TYPE, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres};

use crate::{
    db,
    execution::model::{ExecutionResult, HandlerSpec},
    service,
    util::VERSION,
};

const RESULT_PAGE_SIZE: i32 = 1000;

async fn heartbeat(State(shared_state): State<Pool<Postgres>>) -> Response {
    match db::pool::heartbeat(&shared_state).await {
        Ok(result) if result => (
            StatusCode::OK,
             ErasedJson::pretty(
                serde_json::json!({"heartbeat": result, "platform": "Pardalotus API", "version": VERSION}),
            ),
        ),
        Err(e) => {
            log::error!("Heartbeat failure: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErasedJson::pretty(serde_json::json!({"heartbeat": false, "platform": "Pardalotus API", "version": VERSION})),
            )
        }
        _ => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErasedJson::new(serde_json::json!({"heartbeat": false, "platform": "Pardalotus API", "version": VERSION})),
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
        Ok(result) => (
            StatusCode::OK,
            ErasedJson::pretty(HandlerPage { results: result }),
        )
            .into_response(),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ErasedJson::pretty(serde_json::json!({"error": "can't fetch handlers"})),
        )
            .into_response(),
    }
}

async fn post_function(State(pool): State<Pool<Postgres>>, mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "data" {
            if let Ok(data) = field.text().await {
                let task = HandlerSpec {
                    handler_id: -1,
                    code: data,
                    status: db::handler::HandlerState::Enabled as i32,
                };

                return match service::load_handler(&pool, &task).await {
                    service::TaskLoadResult::Exists { task_id } => (
                        StatusCode::OK,
                        ErasedJson::pretty(
                            serde_json::json!({"status": "already-exists", "task_id": task_id}),
                        ),
                    )
                        .into_response(),
                    service::TaskLoadResult::New { task_id } => (
                        StatusCode::CREATED,
                        ErasedJson::pretty(
                            serde_json::json!({"status": "created", "task-id": task_id}),
                        ),
                    )
                        .into_response(),
                    service::TaskLoadResult::FailedSave() => (
                        StatusCode::BAD_REQUEST,
                        ErasedJson::pretty(serde_json::json!({"status": "failed"})),
                    )
                        .into_response(),
                };
            }
        }
    }

    return (
        StatusCode::BAD_REQUEST,
        ErasedJson::pretty(serde_json::json!({"status": "failed", "reason": "No function supplied. Please check the documentation."})),
    )
        .into_response();
}

async fn get_function_info(
    Path(handler_id): Path<i64>,
    State(pool): State<Pool<Postgres>>,
) -> Response {
    match service::get_handler_by_id(&pool, handler_id).await {
        Some(handler) => (StatusCode::OK, ErasedJson::pretty(handler)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            ErasedJson::pretty(serde_json::json!({"error": "handler not found"})),
        )
            .into_response(),
    }
}

async fn get_function_code(
    Path(handler_id): Path<i64>,
    State(pool): State<Pool<Postgres>>,
) -> Response<String> {
    match service::get_handler_by_id(&pool, handler_id).await {
        Some(handler) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, HeaderValue::from_static("text/javascript"))
            .body(handler.code)
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CONTENT_TYPE, HeaderValue::from_static("text/javascript"))
            .body(String::from(""))
            .unwrap(),
    }
}

#[derive(Serialize)]
struct ResultsPage {
    cursor: i64,
    results: Vec<Value>,
}

#[derive(Deserialize)]
struct ResultQuery {
    cursor: Option<i64>,
}

async fn get_function_results(
    Path(handler_id): Path<i64>,
    Query(query): Query<ResultQuery>,
    State(pool): State<Pool<Postgres>>,
) -> Response {
    let (results, next_cursor) = service::get_results(
        &pool,
        handler_id,
        query.cursor.unwrap_or(-1),
        RESULT_PAGE_SIZE,
        true,
    )
    .await;

    // Convert result JSON strings into result JSON Values for constructing a page.
    // If these don't parse, then ignore them.
    let results: Vec<Value> = results
        .into_iter()
        .filter_map(|x| x.result)
        .filter_map(|r| match serde_json::from_str(&r) {
            Ok(x) => Some(x),
            _ => None,
        })
        .collect();
    let page = ResultsPage {
        results,
        cursor: next_cursor,
    };
    (StatusCode::OK, ErasedJson::pretty(page)).into_response()
}

#[derive(Serialize)]
struct ResultsDebugPage {
    cursor: i64,
    results: Vec<ExecutionResult>,
}

async fn get_function_debug(
    Path(handler_id): Path<i64>,
    Query(query): Query<ResultQuery>,
    State(pool): State<Pool<Postgres>>,
) -> Response {
    let (results, next_cursor) = service::get_results(
        &pool,
        handler_id,
        query.cursor.unwrap_or(-1),
        RESULT_PAGE_SIZE,
        false,
    )
    .await;

    let page = ResultsDebugPage {
        results,
        cursor: next_cursor,
    };
    (StatusCode::OK, ErasedJson::pretty(page)).into_response()
}

pub(crate) async fn run(pool: &Pool<Postgres>) {
    let app = Router::new()
        .route("/functions/", get(list_functions).post(post_function))
        .route("/functions/:handler_id/info", get(get_function_info))
        .route("/functions/:handler_id/code.js", get(get_function_code))
        .route("/functions/:handler_id/results", get(get_function_results))
        .route("/functions/:handler_id/debug", get(get_function_debug))
        .route("/heartbeat", get(heartbeat))
        .with_state(pool.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:6464").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
