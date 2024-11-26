//! Service layer
//! For running and coordinating functions.

use serde_json::Value;
use sqlx::{Error, Pool, Postgres};

use crate::{
    db::{self, event::EventQueueState},
    execution::{
        self,
        model::{Event, ExecutionResult, HandlerSpec},
    },
    local,
    util::hash_data,
};

const EXECUTE_BATCH_SIZE: i32 = 100;

/// List all handlers.
/// For now, assumes that there are enough to fit in memory, and an API response.
pub(crate) async fn list_handlers(pool: &Pool<Postgres>) -> Result<Vec<HandlerSpec>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    db::handler::get_all_enabled_handlers(&mut tx).await
}

/// Load functions from specified directory.
/// These are configured at boot, not directly by a user, so the result is logged.
pub(crate) async fn load_handler_functions_from_disk(
    pool: &Pool<Postgres>,
    path: std::path::PathBuf,
) {
    let tasks = local::load_tasks_from_dir(path);
    for (filename, task) in tasks {
        match load_handler(pool, &task).await {
            TaskLoadResult::New { task_id } => {
                log::info!("Loaded task {} from {}", task_id, &filename)
            }
            TaskLoadResult::Exists { task_id } => {
                log::info!("Task already exists at {} with id {}", &filename, task_id)
            }
            TaskLoadResult::FailedSave() => {
                log::error!("Failed to load task from {}", &filename)
            }
        }
    }
}

pub(crate) enum TaskLoadResult {
    New { task_id: i64 },
    Exists { task_id: i64 },
    FailedSave(),
}

/// Load a function. On creation return New ID, or report that it already exists.
pub(crate) async fn load_handler(pool: &Pool<Postgres>, task: &HandlerSpec) -> TaskLoadResult {
    let hash = hash_data(&task.code);

    log::info!("Load function {}", hash);

    let insert_result =
        db::handler::insert_handler(task, &hash, 0, db::handler::HandlerState::Enabled, pool);

    match insert_result.await {
        Ok((handler_id, true)) => TaskLoadResult::New {
            task_id: handler_id,
        },
        Ok((handler_id, false)) => TaskLoadResult::Exists {
            task_id: handler_id,
        },
        Err(e) => {
            log::error!("Failed to save handler {}: {:?}", hash, e);
            TaskLoadResult::FailedSave()
        },
    }
}

pub(crate) async fn load_events_from_disk(
    pool: &Pool<Postgres>,
    path: std::path::PathBuf,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    let files = local::load_files_from_dir(path)?;

    for (filename, data) in files {
        match serde_json::from_str::<Vec<Value>>(&data) {
            Ok(items) => {
                for item in items {
                    // Parse to break apart array and re-serialize.
                    // Not the most efficient, but this is a cold code path.
                    match serde_json::to_string(&item) {
                        Ok(json) => {
                            if let Some(event) = Event::from_json_value(&json) {
                                // Subject and Object are optional.
                                let subject_entity_id = if let Some(ref id) = event.subject_id {
                                    Some(db::entity::resolve_identifier(id, pool).await?)
                                } else {
                                    None
                                };

                                let object_entity_id = if let Some(ref id) = event.object_id {
                                    Some(db::entity::resolve_identifier(id, pool).await?)
                                } else {
                                    None
                                };

                                // Normalize
                                db::event::insert_event(
                                    &event,
                                    subject_entity_id,
                                    object_entity_id,
                                    EventQueueState::New,
                                    &mut tx,
                                )
                                .await?;
                            } else {
                                log::error!(
                                    "Didn't insert event from file: {}. Input: {}",
                                    filename,
                                    &json
                                );
                            }
                        }
                        Err(e) => {
                            log::error!("Can't serialize event input: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to parse input events: {}", e);
            }
        }
    }

    tx.commit().await?;

    Ok(())
}

#[derive(Debug)]
pub(crate) struct PumpResult {
    events_processed: u32,
    poll_duration: u128,
    execute_duration: u128,
    save_duration: u128,
    total_duration: u128,
    results: usize,
    handlers: usize,
}

pub(crate) async fn drain(pool: &Pool<Postgres>) {
    let mut count = EXECUTE_BATCH_SIZE;

    // Keep going until we get a less-than-full page.
    while count >= EXECUTE_BATCH_SIZE {
        match try_pump(pool, EXECUTE_BATCH_SIZE).await {
            Ok(result) => {
                log::info!(
            "Pumped {} events through {} handlers in {}ms. Got {} results. Poll: {}, execute: {}, save: {}",
            result.events_processed,
            result.handlers,
            result.total_duration,
            result.results,
            result.poll_duration,
            result.execute_duration,
            result.save_duration
        );
                count = result.events_processed as i32;
            }
            Err(e) => {
                log::error!("Failed to poll queue. Error: {:?}", e);
                // Terminate loop.
                count = 0;
            }
        }
    }
}

/// Poll for a batch of inputs, run handler functions.
/// Does not necessarily consume all messages on the queue.
pub(crate) async fn try_pump(pool: &Pool<Postgres>, batch_size: i32) -> Result<PumpResult, Error> {
    let start_poll = std::time::Instant::now();

    let mut tx = pool.begin().await?;

    let events = db::event::poll(batch_size, &mut tx).await?;
    log::debug!("Polled {} from Event queue", events.len());

    // Get all handlers. Do so from inside the transaction so there's a
    // consistent view of the handlers table. If it becomes necessary to chunk
    // into batches of handlers in future, this will be important.
    let handlers: Vec<HandlerSpec> = db::handler::get_all_enabled_handlers(&mut tx).await?;

    let start_execution = std::time::Instant::now();
    let results = execution::run::run_all(&handlers, &events);

    let start_save = std::time::Instant::now();
    db::handler::save_results(&results, &mut tx).await?;

    log::debug!("Saved {} execution results", results.len());

    tx.commit().await?;
    let finish = std::time::Instant::now();

    Ok(PumpResult {
        events_processed: events.len() as u32,
        handlers: handlers.len(),
        results: results.len(),
        poll_duration: start_execution.duration_since(start_poll).as_millis(),
        execute_duration: start_save.duration_since(start_execution).as_millis(),
        save_duration: finish.duration_since(start_save).as_millis(),
        total_duration: finish.duration_since(start_poll).as_millis(),
    })
}

/// Get Handler Spec by ID, or None.
pub(crate) async fn get_handler_by_id(
    pool: &Pool<Postgres>,
    handler_id: i64,
) -> Option<HandlerSpec> {
    match db::handler::get_by_id(pool, handler_id).await {
        Ok(handler_id) => Some(handler_id),
        Err(e) => {
            log::error!("Didn't find handler id {}, error: {:?}", handler_id, e);
            None
        }
    }
}

/// Get a page of results, plus a cursor for the next page.
/// If filter_successful is true, only return successful results.
pub(crate) async fn get_results(
    pool: &Pool<Postgres>,
    handler_id: i64,
    cursor: i64,
    page_size: i32,
    filter_successful: bool,
) -> (Vec<ExecutionResult>, i64) {
    let results: Result<Vec<ExecutionResult>, sqlx::Error> = if filter_successful {
        db::handler::get_success_results(pool, handler_id, cursor, page_size).await
    } else {
        db::handler::get_all_results(pool, handler_id, cursor, page_size).await
    };

    match results {
        Ok(results) => {
            let next_cursor = results.last().map(|x| x.result_id).unwrap_or(-1);
            (results, next_cursor)
        }
        Err(err) => {
            log::error!(
                "Error retrieving results for handler id: {}, error: {:?}",
                handler_id,
                err
            );
            (vec![], -1)
        }
    }
}
