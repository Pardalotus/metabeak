use execution::run::{Input, TaskSpec};

mod execution;

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting metabeak");

    let global_context = execution::run::GlobalContext::new();

    let tasks: Vec<execution::run::TaskSpec> = vec![TaskSpec {
        user_id: 1,
        id: 2,
        code: String::from("function x() {return KEY ;}; x();"),
    }];

    let inputs: Vec<Input> = vec![Input {
        data: String::from("hello"),
    }];

    let result = execution::run::run_all(&tasks, &inputs);

    println!("Results: {:?}", result);

    log::info!("Exit metabeak");
}
