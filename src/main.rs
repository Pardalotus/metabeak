use execution::run::Input;
use serde_json::json;
use std::path::PathBuf;
use structopt::StructOpt;
mod execution;

#[derive(Debug, StructOpt)]
#[structopt(name = "metabeak", about = "Pardalotus Metabeak API.")]
struct Options {
    /// Local directory of tasks.
    #[structopt(
        long,
        parse(from_os_str),
        help("local directory path to load functions on startup")
    )]
    load: Option<PathBuf>,
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let opt = Options::from_args();

    let mut tasks: Vec<execution::run::TaskSpec> = vec![];

    if let Some(load_dir) = opt.load {
        log::info!(
            "Reading functions from {}",
            load_dir.clone().into_os_string().into_string().unwrap()
        );
        tasks.extend(execution::run::load_tasks_from_dir(load_dir));
    }

    log::info!("Starting metabeak");

    execution::run::GlobalContext::new();

    // Dummy inputs
    let mut inputs: Vec<Input> = vec![
        Input {
            data: json!({"input": "data1"}),
        },
        Input {
            data: json!({"input": "data2"}),
        },
        Input {
            data: json!({"input": "data3"}),
        },
    ];

    let results = execution::run::run_all(&tasks, &inputs);

    log::info!("Got {} results", results.len());

    for result in results.iter() {
        log::info!("Result:");
        log::info!("Error: {:?}", result.error);
        log::info!("Output: {:?}", result.output);
    }

    log::info!("Exit metabeak");
}
