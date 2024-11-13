use std::path::PathBuf;
use structopt::StructOpt;
mod database;
mod execution;
mod local;
mod service;
use tokio;

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
        help("Run a single cycle of the pump and exit. This will poll from the inputs and run functions.")
    )]
    execute_one: bool,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let opt = Options::from_args();

    // Boot the database.
    let db_pool = database::get_pool().await.unwrap();

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

    // Run Optional features.
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

    // Run executor.
    if opt.execute_one {
        log::info!("Starting executor...");

        // For now just a sungle poll and exit.
        service::pump(&db_pool).await;
        log::info!("Finish executor.");
    }

    // Gracefully closing the pool avoids extraneous errors in the PostgreSQL log.
    database::close_pool(&db_pool).await;
    log::info!("Exit metabeak");
}
