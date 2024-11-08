use std::fs;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use v8::{Function, HandleScope, Local, Object, V8};

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
pub(crate) struct RunResult {
    // Array of results from the run.
    pub(crate) output: Option<serde_json::Value>,

    // Error string, if execution failed.
    pub(crate) error: Option<String>,
}

/// Run all tasks against all inputs.
/// Create an isolated environment for each distinct user.
pub(crate) fn run_all(tasks: &[TaskSpec], inputs: &[Input]) -> Vec<RunResult> {
    log::info!("Run {} tasks against {} inputs", tasks.len(), inputs.len());

    let mut results: Vec<RunResult> = vec![];

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
            set_variable_from_json(task_scope, task_proxy, "environment", &environment_json);

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
                            error(
                                &mut results,
                                "'f' was not a function. Check you have don't have a variable named `f`.",
                            );
                        } else {
                            // Guarded by enclosing if, so this is safe.
                            let as_f = query_function.cast::<Function>();

                            // Now execute for each input.
                            for input in inputs {
                                let input_handle = marshal_task_input(task_scope, &input.data);

                                let run = as_f.call(task_scope, query_function, &[input_handle]);

                                if let Some(result) = run {
                                    let result_json = v8::json::stringify(task_scope, result)
                                        .unwrap()
                                        .to_rust_string_lossy(task_scope);

                                    if result_json.eq(&"undefined") {
                                        error(&mut results, "Function didn't return a value. Check for a `return` statement.");
                                    } else if let Ok(result) = serde_json::from_str(&result_json) {
                                        // We have no expectations of the format of the result at this stage, just that it should parse.
                                        results.push(RunResult {
                                            output: Some(result),
                                            error: None,
                                        })
                                    } else {
                                        error(
                                            &mut results,
                                            "Failed to parse result from function.",
                                        );
                                    }
                                } else {
                                    error(&mut results, "Failed to run the function.");
                                }
                            }
                        }
                    } else {
                        error(&mut results, "Didn't find named function.");
                    }
                } else {
                    error(&mut results, "Failed to compile code.");
                }
            } else {
                error(&mut results, "Failed to load code.");
            }
        }
    }

    results
}

/// Marshal a Serde JSON input a parsed value in the context.
/// Return the handle.
fn marshal_task_input<'s>(
    scope: &mut HandleScope<'s>,
    data: &serde_json::Value,
) -> Local<'s, v8::Value> {
    // Marshall input as a string, deserialize in the VM.
    let json_input = serde_json::to_string(data).unwrap();
    let marshalled_json_input = v8::String::new(scope, &json_input).unwrap();
    v8::json::parse(scope, marshalled_json_input).unwrap()
}

/// Set a variable on the given object via its handle.
/// Object the value should be expressed as a JSON value string.
fn set_variable_from_json(
    scope: &mut HandleScope,
    object: Local<'_, Object>,
    key: &str,
    json_val: &str,
) {
    let key_marshalled = v8::String::new(scope, key).unwrap();
    let value_marshalled = v8::String::new(scope, &json_val).unwrap();
    let value_parsed = v8::json::parse(scope, value_marshalled).unwrap();
    object.set(scope, key_marshalled.into(), value_parsed);
}

/// Push an error message to the results.
fn error(results: &mut Vec<RunResult>, message: &str) {
    results.push(RunResult {
        output: None,
        error: Some(String::from(message)),
    });
}

/// Load tasks from JS files in directory.
pub(crate) fn load_tasks_from_dir(load_dir: std::path::PathBuf) -> Vec<TaskSpec> {
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
