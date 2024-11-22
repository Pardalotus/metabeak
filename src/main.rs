use metadata_assertion::crossref::{self};
use std::path::PathBuf;
use structopt::StructOpt;

mod db;
mod event_extraction;
mod execution;
mod local;
mod metadata_assertion;
mod service;
mod util;

#[derive(Debug, StructOpt)]
#[structopt(name = "metabeak", about = "Pardalotus Metabeak API.")]
struct Options {
    /// Load tasks from directory at path
    #[structopt(
        long,
        parse(from_os_str),
        help("On startup, load handler functions from directory at path")
    )]
    load_handlers: Option<PathBuf>,

    /// Load tasks from directory at path
    #[structopt(
        long,
        parse(from_os_str),
        help("On startup, load events from directory at path. Each file should contain an array of events.")
    )]
    load_events: Option<PathBuf>,

    #[structopt(
        long,
        help("Execute handlers over all Events in the queue. Exit when queue is empty.")
    )]
    execute: bool,

    #[structopt(
        long,
        help("Fetch all Crossref metadata assertions since the last run.")
    )]
    fetch_crossref: bool,

    #[structopt(long, help("Process the entire Metadata Assertion queue to produce Events. Exit when queue is empty."))]
    extract: bool,
}

/// Run the main function.
/// The sequencing of operations is in order of occurrence in the pipeline.
/// This means if you select the right options, the output of one stage will be available for the next.
#[tokio::main]
async fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let opt = Options::from_args();

    // Boot the database.
    let db_pool = db::pool::get_pool().await.unwrap();

    // Boot the v8 environment, as it's used in both validation and execution of functions.
    execution::run::init();

    // Run Optional features.
    if let Some(path) = opt.load_handlers {
        log::info!(
            "Reading functions from {}",
            path.clone().into_os_string().into_string().unwrap()
        );
        service::load_handler_functions_from_disk(&db_pool, path).await;
    }

    if let Some(path) = opt.load_events {
        log::info!(
            "Reading events from {}",
            path.clone().into_os_string().into_string().unwrap()
        );
        match service::load_events_from_disk(&db_pool, path).await {
            Ok(()) => {
                log::info!("Loaded events");
            }
            Err(e) => {
                log::error!("Didn't load events: {}", e);
            }
        }
    }

    if opt.fetch_crossref {
        log::info!("Poll Crossref for new metadata...");
        match crossref::metadata_agent::pump_metadata(&db_pool).await {
            Ok(_) => {
                log::info!("Finished polling Crossref for metadata.");
            }
            Err(e) => {
                log::error!("Error polling Crossref for metadata: {:?}", e);
            }
        }
    }

    if opt.extract {
        log::info!("Processing metadata to extract events...");
        match event_extraction::service::drain(&db_pool).await {
            Ok(_) => {
                log::info!("Finished extracting events.");
            }
            Err(e) => {
                log::error!("Error extracting events: {:?}", e);
            }
        }
    }

    // Run executor.
    if opt.execute {
        log::info!("Starting executor...");
        service::drain(&db_pool).await;
        log::info!("Finish executor.");
    }

    // Gracefully closing the pool avoids extraneous errors in the PostgreSQL log.
    db::pool::close_pool(&db_pool).await;
    log::info!("Exit metabeak");
}
