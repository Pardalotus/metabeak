use metadata_assertion::crossref::{self};
use std::path::PathBuf;
use std::{env, process::exit};
use structopt::StructOpt;
use tokio::task::JoinSet;
mod api;
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

    #[structopt(
        long,
        help("Fetch all Crossref metadata assertions matching given filter as secondary metadata assertions (i.e. does not trigger events). Filter e.g. 'from-deposit-date:2021-01-01,until-deposit-date:2021-01-02'.")
    )]
    fetch_crossref_secondary: Option<String>,

    #[structopt(long, help("Process the entire Metadata Assertion queue to produce Events. Exit when queue is empty."))]
    extract: bool,

    #[structopt(long, help("Start the API server and block."))]
    api: bool,
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

    let uri = env::var("DB_URI");
    if let Err(_) = uri {
        log::error!("DB_URI not supplied");
        exit(1);
    }

    // Boot the database.
    let db_pool = db::pool::get_pool(uri.unwrap()).await.unwrap();

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
        match crossref::metadata_agent::poll_newly_indexed_data(&db_pool).await {
            Ok(_) => {
                log::info!("Finished polling Crossref for metadata.");
            }
            Err(e) => {
                log::error!("Error polling Crossref for metadata: {:?}", e);
            }
        }
    }

    if let Some(filter) = opt.fetch_crossref_secondary {
        log::info!(
            "Poll Crossref for secondary metadata assertions with filter {}...",
            filter
        );

        match crossref::metadata_agent::fetch_secondary_metadata_with_filter(&db_pool, filter).await
        {
            Ok(_) => {
                log::info!("Finished polling Crossref for secondary metadata.");
            }
            Err(e) => {
                log::error!("Error polling Crossref for secondary metadata: {:?}", e);
            }
        }
    }

    if opt.extract {
        let mut set = JoinSet::new();

        for i in 0..5 {
            log::info!("Start extract task {}", i);
            let db_pool = db_pool.clone();
            set.spawn(async move {
                log::info!("Processing metadata to extract events...");
                match event_extraction::service::drain(&db_pool).await {
                    Ok(_) => {
                        log::info!("Finished extracting events.");
                    }
                    Err(e) => {
                        log::error!("Error extracting events: {:?}", e);
                    }
                };
            });
        }

        log::info!("Wait for extract tasks to complete.");
        set.join_all().await;
        log::info!("All extract tasks complete.");
    }

    // Run executor.
    if opt.execute {
        log::info!("Starting executor...");
        service::drain(&db_pool).await;
        log::info!("Finish executor.");
    }

    // Run API server.
    if opt.api {
        log::info!("Starting API server...");
        api::run(&db_pool).await;
    }

    // Gracefully closing the pool avoids extraneous errors in the PostgreSQL log.
    db::pool::close_pool(&db_pool).await;
    log::info!("Exit metabeak");
}
