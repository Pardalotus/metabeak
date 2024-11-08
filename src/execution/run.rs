use std::fs;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use v8::{Function, V8};

pub(crate) struct GlobalContext {}

/// Global execution context for v8.
impl GlobalContext {
    pub(crate) fn new() -> GlobalContext {
        let platform = v8::new_default_platform(0, false).make_shared();
        V8::initialize_platform(platform);
        V8::initialize();

        GlobalContext {}
    }
}

// This is provided by Cargo at build time, so complied as a static string.
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

/// Environment passed into each function execution.
#[derive(Serialize, Deserialize)]
pub(crate) struct Global {
    environment: String,
    version: String,
}

impl Global {
    fn build() -> Global {
        Global {
            environment: String::from("Pardalotus Metabeak"),
            version: String::from(VERSION),
        }
    }

    fn json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

/// A task to be run.
#[derive(Debug)]
pub(crate) struct TaskSpec {
    /// User ID executing the TaskSpec. This may not correspond to an entity, but is reserved for future use.
    pub(crate) user_id: u64,

    /// JavaScript code that must contain a function named 'f'.
    pub(crate) code: String,
}

/// Input data for a task run.
#[derive(Debug)]
pub(crate) struct Input {
    pub(crate) data: serde_json::Value,
}

/// Result from a task run.
#[derive(Debug)]
pub(crate) struct Result {
    // TODO should be serde_json?
    pub(crate) output: Option<String>,

    // Error string, if execution failed.
    pub(crate) error: Option<String>,
}

/// Run all tasks against all inputs.
/// Create an isolated environment for each distinct user.
pub(crate) fn run_all(tasks: &[TaskSpec], inputs: &[Input]) -> Vec<Result> {
    log::info!("Run {} tasks against {} inputs", tasks.len(), inputs.len());

    let mut results: Vec<Result> = vec![];

    // Representation of the global 'environment' variable provided to all function invocations.
    let environment_json = Global::build().json();

    // Isolated environment for each user.
    // Each environment needs data marshaling into it, so there's repeated creation of handles and conversion of data.
    let by_user_id = tasks.iter().map(|t| (t.user_id, t)).into_group_map();

    for (user_id, task_specs) in by_user_id.iter() {
        log::info!("Running {} tasks for user_id {}", task_specs.len(), user_id);

        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut v8::HandleScope::new(isolate);

        // Each task associated with the user.
        for task_spec in task_specs.iter() {
            let task_context = v8::Context::new(scope, Default::default());
            let task_scope = &mut v8::ContextScope::new(scope, task_context);
            let task_proxy = task_context.global(task_scope);

            // Set the global 'environment' variable.
            let environment_key = v8::String::new(task_scope, "environment").unwrap();
            let environment_value_marshalled =
                v8::String::new(task_scope, &environment_json).unwrap();
            let environment_value_parsed =
                v8::json::parse(task_scope, environment_value_marshalled).unwrap();
            task_proxy.set(task_scope, environment_key.into(), environment_value_parsed);

            // Compile the function associated with the task.
            if let Some(code) = v8::String::new(task_scope, &task_spec.code) {
                if let Some(script) = v8::Script::compile(task_scope, code, None) {
                    // First execute the function in the task's scope.
                    // This will result in variables being registered, including the function 'f'.
                    script.run(task_scope).unwrap();

                    // Now we can look for the function that was registered.
                    let function_key = v8::String::new(task_scope, "f").unwrap();
                    if let Some(query_function) = task_proxy.get(task_scope, function_key.into()) {
                        if !query_function.is_function() {
                            results.push(Result {
                                output: None,
                                error: Some(String::from("'f' was not a function.")),
                            });
                        } else {
                            // Guarded by enclosing if, so this is safe.
                            let as_f = query_function.cast::<Function>();

                            // Now execute for each input.
                            for input in inputs {
                                // Marshall input as a string, deserialize in the VM.
                                let json_input = serde_json::to_string(&input.data).unwrap();
                                let marshalled_json_input =
                                    v8::String::new(task_scope, &json_input).unwrap();
                                let parsed_input =
                                    v8::json::parse(task_scope, marshalled_json_input).unwrap();

                                let run = as_f.call(task_scope, query_function, &[parsed_input]);

                                if let Some(result) = run {
                                    let result_str = result.to_rust_string_lossy(task_scope);

                                    results.push(Result {
                                        output: Some(result_str),
                                        error: None,
                                    })
                                } else {
                                    results.push(Result {
                                        output: None,
                                        error: Some(String::from("Failed to run code.")),
                                    });
                                }
                            }
                        }
                    } else {
                        results.push(Result {
                            output: None,
                            error: Some(String::from("Didn't find named function.")),
                        });
                    }
                } else {
                    results.push(Result {
                        output: None,
                        error: Some(String::from("Failed to compile code.")),
                    });
                }
            } else {
                results.push(Result {
                    output: None,
                    error: Some(String::from("Failed to load code.")),
                });
            }
        }
    }

    results
}

/// Load tasks from JS files in directory.
pub(crate) fn load(load_dir: std::path::PathBuf) -> Vec<TaskSpec> {
    let mut result = vec![];

    match fs::read_dir(load_dir) {
        Err(e) => {
            log::error!("Can't load functions from disk: {}", e);
        }
        Ok(listing) => {
            for file in listing {
                match file {
                    Ok(entry) => {
                        if entry.path().is_file() {
                            match fs::read_to_string(entry.path()) {
                                Err(e) => log::error!("Can't read file: {}", e),
                                Ok(content) => {
                                    result.push(TaskSpec {
                                        user_id: 0,
                                        code: content,
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Can't load file: {}", e);
                    }
                }
            }
        }
    }

    result
}
