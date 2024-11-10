//! Run functions in V8.
//! For each function, spin up a V8 environment and execute the function.

use std::fs;

use serde::{Deserialize, Serialize};
use v8::{Context, Function, HandleScope, Local, Object, Value, V8};

/// Initialize the V8 environment.
pub(crate) fn init() {
    let platform = v8::new_default_platform(0, false).make_shared();
    V8::initialize_platform(platform);
    V8::initialize();
}

// This is provided by Cargo at build time, so complied as a static string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    /// ID of the task, to allow collation of results.
    pub(crate) task_id: u64,

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
    /// ID of the task.
    pub(crate) task_id: u64,

    /// Array of results from the run.
    pub(crate) output: Option<serde_json::Value>,

    /// Error string, if execution failed.
    pub(crate) error: Option<String>,
}

/// Run all tasks against all inputs.
/// Create an isolated environment for each distinct user.
pub(crate) fn run_all(tasks: &[TaskSpec], inputs: &[Input]) -> Vec<RunResult> {
    log::info!("Run {} tasks against {} inputs", tasks.len(), inputs.len());

    let mut results: Vec<RunResult> = vec![];

    // Representation of the global 'environment' variable provided to all function invocations.
    let environment_json = Global::build().json();

    // Isolated environment for each task, re-used for all input data.
    for task_spec in tasks.iter() {
        log::info!("Running task id {}", task_spec.task_id);

        let isolate = &mut v8::Isolate::new(Default::default());
        let handle_scope = &mut v8::HandleScope::new(isolate);

        // Each task associated with the user.
        let task_context = v8::Context::new(handle_scope, Default::default());
        let task_scope = &mut v8::ContextScope::new(handle_scope, task_context);
        let task_proxy = task_context.global(task_scope);

        // Set the global 'environment' variable.
        set_variable_from_json(task_scope, task_proxy, "environment", &environment_json);

        // Load the script from the task spec and execute it.
        // The script should define a function called 'f', which we'll retrieve from the scope.
        // This means we don't need to retain a direct handle to the script itself once it's executed.
        // On failure, log exception message to results.
        let ok: bool = load_script(task_spec, &mut results, task_scope);

        // Now retrieve the function from the context.
        if ok {
            if let Some((function_as_f, function_as_v)) =
                get_f_function(&task_spec, &mut results, task_scope, task_proxy)
            {
                // Execute f for each input.
                for input in inputs {
                    let input_handle = marshal_task_input(task_scope, &input.data);

                    // Run in a TryCatch so we can retrieve error messages.
                    let mut try_catch_scope = v8::TryCatch::new(task_scope);
                    let run =
                        function_as_f.call(&mut try_catch_scope, function_as_v, &[input_handle]);

                    match run {
                        None => {
                            // Run failed. Try to report the exception.
                            if let Some(ex) = try_catch_scope.exception() {
                                let message = ex.to_rust_string_lossy(&mut try_catch_scope);
                                report_error(
                                    task_spec,
                                    &mut results,
                                    format!("Failed to run the function. Exception: {}", message),
                                );
                            } else {
                                report_error(
                                    task_spec,
                                    &mut results,
                                    String::from(
                                        "Failed to run the function, no exception available.",
                                    ),
                                );
                            }
                        }
                        Some(result) => {
                            // Run succeeded.
                            report_result(task_spec, &mut results, result, &mut try_catch_scope);
                        }
                    }
                }
            }
        }
    }

    results
}

/// Given the output of a function run, parse it and append the result to the results list.
fn report_result(
    task_spec: &TaskSpec,
    results: &mut Vec<RunResult>,
    result: Local<'_, Value>,
    scope: &mut HandleScope<'_, Context>,
) {
    let result_json = v8::json::stringify(scope, result)
        .unwrap()
        .to_rust_string_lossy(scope);

    // Handle 'undefined' as a special case.
    if result_json.eq(&"undefined") {
        report_error(
            task_spec,
            results,
            String::from("Function didn't return a value. Check for a `return` statement."),
        );
    } else if let Ok(result) = serde_json::from_str(&result_json) {
        // We have no expectations of the format of the result at this stage, just that it should parse.
        results.push(RunResult {
            task_id: task_spec.task_id,
            output: Some(result),
            error: None,
        })
    } else {
        report_error(
            task_spec,

             results,
            String::from("Failed to parse result from function. Did you return objects that can't be represented in JSON?"),
        );
    }
}

/// Push an error message to the results.
fn report_error(task_spec: &TaskSpec, results: &mut Vec<RunResult>, message: String) {
    results.push(RunResult {
        task_id: task_spec.task_id,
        output: None,
        error: Some(String::from(message)),
    });
}

/// From a Context in which a script has already been loaded and executed, leaving a function named 'f'.
/// Retrieve that function and return it.
/// Returns the function as a Value and cast to a Function, as required by the V8 function invocation API.
/// A little strange, but lets us keep the separation of concerns, and handle both "does f exist" and "is f a function".
fn get_f_function<'s>(
    task_spec: &TaskSpec,
    results: &mut Vec<RunResult>,
    task_scope: &mut HandleScope<'s>,
    task_proxy: Local<'s, Object>,
) -> Option<(Local<'s, Function>, Local<'s, Value>)> {
    // Now we can look for the function that was registered.
    let function_key = v8::String::new(task_scope, "f").unwrap();

    if let Some(query_function) = task_proxy.get(task_scope, function_key.into()) {
        if !query_function.is_function() {
            report_error(            task_spec,

                results,
                String::from(
                    "'f' was not a function. Check you have don't have a conflicting variable named `f`.",
                ),
            );
            None
        } else {
            // Guarded by enclosing if, so this is safe.
            Some((query_function.cast::<Function>(), query_function))
        }
    } else {
        report_error(
            task_spec,
            results,
            String::from("Didn't find named function."),
        );
        None
    }
}

fn load_script(
    task_spec: &TaskSpec,
    results: &mut Vec<RunResult>,
    task_scope: &mut HandleScope<'_, Context>,
) -> bool {
    if let Some(code) = v8::String::new(task_scope, &task_spec.code) {
        if let Some(script) = v8::Script::compile(task_scope, code, None) {
            let mut try_catch_scope = v8::TryCatch::new(task_scope);

            let run = script.run(&mut try_catch_scope);

            match run {
                None => {
                    if let Some(ex) = try_catch_scope.exception() {
                        let message = ex.to_rust_string_lossy(&mut try_catch_scope);
                        report_error(
                            task_spec,
                            results,
                            format!("Failed to load the function. Exception: {}", message),
                        );
                        false
                    } else {
                        report_error(
                            task_spec,
                            results,
                            String::from("Failed to load the function, no exception available."),
                        );
                        false
                    }
                }
                Some(_) => {
                    // We don't care about the result, just that it executed without error.
                    true
                }
            }
        } else {
            report_error(task_spec, results, String::from("Failed to compile code."));
            false
        }
    } else {
        report_error(task_spec, results, String::from("Failed to load code."));
        false
    }
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
    let value_marshalled = v8::String::new(scope, json_val).unwrap();
    let value_parsed = v8::json::parse(scope, value_marshalled).unwrap();
    object.set(scope, key_marshalled.into(), value_parsed);
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
                                        task_id: 0,
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
