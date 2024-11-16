//! Service layer
//! For running and coordinating functions.

use serde_json::Value;
use sha1::{Digest, Sha1};
use sqlx::{Error, Pool, Postgres};

use crate::{
    db::{self, event::EventQueueState},
    execution::{
        self,
        model::{Event, HandlerSpec},
    },
    local,
};

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
            TaskLoadResult::Exists => log::info!("Task already exists at {}", &filename),
            TaskLoadResult::FailedSave() => {
                log::error!("Failed to load task from {}", &filename)
            }
        }
    }
}

/// Hash for task for uniqueness in the database.
/// Currently based only on the code.
fn task_hash(task: &HandlerSpec) -> String {
    let mut hasher = Sha1::new();
    hasher.update(&task.code);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("")
}

enum TaskLoadResult {
    New { task_id: u64 },
    Exists,
    FailedSave(),
}

/// Load a function. On creation return New ID, or report that it already exists.
async fn load_handler(pool: &Pool<Postgres>, task: &HandlerSpec) -> TaskLoadResult {
    let hash = task_hash(task);

    log::info!("Load function {}", hash);

    let insert_result =
        db::handler::insert_handler(task, &hash, 0, db::handler::HandlerState::Enabled, pool);

    match insert_result.await {
        Ok(handler_id) => TaskLoadResult::New {
            task_id: handler_id,
        },
        Err(e) => match e {
            sqlx::Error::RowNotFound => TaskLoadResult::Exists,
            _ => {
                log::error!("Failed to save handler {}: {:?}", hash, e);
                TaskLoadResult::FailedSave()
            }
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
                                    Some(db::entity::resolve_identifier(&id, pool).await?)
                                } else {
                                    None
                                };

                                let object_entity_id = if let Some(ref id) = event.object_id {
                                    Some(db::entity::resolve_identifier(&id, pool).await?)
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
    inputs_processed: u32,
    poll_duration: u128,
    execute_duration: u128,
    save_duration: u128,
    total_duration: u128,
    results: usize,
    handlers: usize,
}

pub(crate) async fn pump(pool: &Pool<Postgres>) {
    match try_pump(pool).await {
        Ok(result) => log::info!(
            "Pumped {} events through {} handlers in {}ms. Got {} results. Poll: {}, execute: {}, save: {}",
            result.inputs_processed,
            result.handlers,
            result.total_duration,
            result.results,
            result.poll_duration,
            result.execute_duration,
            result.save_duration
        ),
        Err(e) => {
            log::error!("Failed to poll queue. Error: {:?}", e);
        }
    }
}

/// Poll for a batch of inputs, run functions.
/// Does not necessarily consume all messages on the queue.
pub(crate) async fn try_pump(pool: &Pool<Postgres>) -> Result<PumpResult, Error> {
    let start_poll = std::time::Instant::now();

    let mut tx = pool.begin().await?;

    let inputs = db::event::poll(1000, &mut tx).await?;

    // Get all handlers. Do so from inside the transaction so there's a
    // consistent view of the handlers table. If it becomes necessary to chunk
    // into batches of handlers in future, this will be important.
    let handlers: Vec<HandlerSpec> = db::handler::all_enabled_handlers(&mut tx).await?;

    let start_execution = std::time::Instant::now();
    let results = execution::run::run_all(&handlers, &inputs);

    let start_save = std::time::Instant::now();
    db::handler::save_results(&results, &mut tx).await?;

    log::info!("Got {} results", results.len());

    tx.commit().await?;
    let finish = std::time::Instant::now();

    Ok(PumpResult {
        inputs_processed: inputs.len() as u32,
        handlers: handlers.len(),
        results: results.len(),
        poll_duration: start_execution.duration_since(start_poll).as_millis(),
        execute_duration: start_save.duration_since(start_execution).as_millis(),
        save_duration: finish.duration_since(start_save).as_millis(),
        total_duration: finish.duration_since(start_poll).as_millis(),
    })
}