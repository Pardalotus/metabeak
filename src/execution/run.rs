//! Run functions in V8.
//! For each function, spin up a V8 environment and execute the function.

use std::fs;

use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use v8::{Context, Function, HandleScope, Local, Object, V8};

use crate::database::{EventAnalyzer, MetadataSource};

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

/// A handler function to be run.
#[derive(Debug, FromRow)]
pub(crate) struct HandlerSpec {
    /// ID of the handler, to allow collation of results.
    /// -1 for undefined (e.g. for testing)
    pub(crate) handler_id: i64,

    /// JavaScript code that must contain a function named 'f'.
    pub(crate) code: String,
}

/// Input data for a handler function run.
/// The analyzer and source fields are not stored in the `json` field.
#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) event_id: i64,

    pub(crate) analyzer: EventAnalyzer,

    pub(crate) source: MetadataSource,

    // Remainder of the JSON structure.
    // See DR-0012.
    pub(crate) json: String,
}

impl Event {
    // Serialize to a public JSON representation, with all fields present.
    pub(crate) fn to_json_value(&self) -> Option<String> {
        let analyzer_value = serde_json::Value::String(self.analyzer.to_str_value());
        let source_value = serde_json::Value::String(self.source.to_str_value());

        match serde_json::from_str::<serde_json::Value>(&self.json) {
            Ok(data) => match data {
                serde_json::Value::Object(mut data_obj) => {
                    data_obj["analyzer"] = analyzer_value;
                    data_obj["source"] = source_value;

                    if let Ok(json) = serde_json::to_string(&serde_json::Value::Object(data_obj)) {
                        Some(json)
                    } else {
                        // Highly unlikely.
                        log::error!("Failed to serialize JSON.");
                        None
                    }
                }
                _ => {
                    log::error!("Got unexpected type for JSON object: {}", &self.json);
                    None
                }
            },
            Err(e) => {
                log::error!(
                    "Failed to parse Event. Error: {:?}. Input: {}",
                    e,
                    &self.json
                );
                None
            }
        }
    }

    /// Load a JSON event from the public JSON representation.
    /// None if there was a problem parsing it.
    /// This clones subfields of the JSON Value, and is on a hot path. Candidate for optimisation if needed.
    pub(crate) fn from_json_value(input: &str) -> Option<Event> {
        match serde_json::from_str::<serde_json::Value>(&input) {
            Ok(data) => match data {
                serde_json::Value::Object(data_obj) => {
                    let analyzer_str = data_obj.get("analyzer")?.as_str().unwrap_or("UNKNOWN");
                    let source_str = data_obj.get("source")?.as_str().unwrap_or("UNKNOWN");
                    let analyzer = EventAnalyzer::from_str_value(analyzer_str);
                    let source = MetadataSource::from_str_value(source_str);

                    // Defaults to -1 (i.e. unassigned), so we can load events for insertion into the database.
                    // Events may be submitted without IDs, and
                    // they're assigned by the database on insertion.
                    let event_id: i64 = match data_obj.get("event_id") {
                        Some(value) => value.as_i64().unwrap_or(-1),
                        None => -1,
                    };

                    let mut normalized_event = serde_json::Map::new();
                    for field in data_obj.keys().into_iter() {
                        if !(field.eq("analyzer") || field.eq("source")) {
                            if let Some(obj) = data_obj.get(field) {
                                normalized_event.insert(field.clone(), obj.clone());
                            }
                        }
                    }
                    if let Ok(json) =
                        serde_json::to_string(&serde_json::Value::Object(normalized_event))
                    {
                        Some(Event {
                            event_id,
                            analyzer,
                            source,
                            json,
                        })
                    } else {
                        // Highly unlikely.
                        log::error!("Failed to serialize JSON.");
                        None
                    }
                }
                _ => {
                    log::error!("Got unexpected type for JSON object: {}", input);
                    None
                }
            },
            Err(e) => {
                log::error!("Failed to parse Event. Error: {:?}. Input: {}", e, input);
                None
            }
        }
    }
}

/// Result from a handler function run.
/// A handler function returns an array of results. There will be one of these objects per entry.
#[derive(Debug)]
pub(crate) struct RunResult {
    /// ID of the handler function used.
    pub(crate) handler_id: i64,

    /// ID of the event it was triggered from.
    pub(crate) event_id: i64,

    /// Single JSON object.
    pub(crate) output: Option<String>,

    /// Error string, if execution failed.
    pub(crate) error: Option<String>,
}

/// Run all tasks against all inputs.
/// Create an isolated environment for each distinct user.
pub(crate) fn run_all(handlers: &[HandlerSpec], inputs: &[Event]) -> Vec<RunResult> {
    log::info!(
        "Run {} tasks against {} inputs",
        handlers.len(),
        inputs.len()
    );

    let mut results: Vec<RunResult> = vec![];

    // Representation of the global 'environment' variable provided to all function invocations.
    let environment_json = Global::build().json();

    // Isolated environment for each task, re-used for all input data.
    for handler_spec in handlers.iter() {
        log::info!("Running task id {}", handler_spec.handler_id);

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
        let ok: bool = load_script(handler_spec, &mut results, task_scope);

        // Now retrieve the function from the context.
        if ok {
            if let Some((function_as_f, function_as_v)) =
                get_f_function(&handler_spec, &mut results, task_scope, task_proxy)
            {
                // Execute f for each input.
                for input in inputs {
                    let input_handle = marshal_task_input(task_scope, &input.json);

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
                                    handler_spec,
                                    input.event_id,
                                    &mut results,
                                    format!("Failed to run the function. Exception: {}", message),
                                );
                            } else {
                                report_error(
                                    handler_spec,
                                    input.event_id,
                                    &mut results,
                                    String::from(
                                        "Failed to run the function, no exception available.",
                                    ),
                                );
                            }
                        }
                        Some(result) => {
                            // Run succeeded. Expect an array of results in a
                            // JSON object, which will be translated into
                            // individual Result objects.
                            report_result_success(
                                handler_spec,
                                input.event_id,
                                &mut results,
                                result,
                                &mut try_catch_scope,
                            );
                        }
                    }
                }
            }
        }
    }

    results
}

/// Given the output of a function run, parse it and append the result to the results list.
fn report_result_success(
    task_spec: &HandlerSpec,
    event_id: i64,
    results: &mut Vec<RunResult>,
    result: Local<'_, v8::Value>,
    scope: &mut HandleScope<'_, Context>,
) {
    let result_json = v8::json::stringify(scope, result)
        .unwrap()
        .to_rust_string_lossy(scope);

    // Handle 'undefined' as a special case.
    if result_json.eq(&"undefined") {
        report_error(
            task_spec,
            event_id,
            results,
            String::from("Function didn't return a value. Check for a `return` statement."),
        );
    } else if let Ok(result_array) = serde_json::from_str::<Vec<serde_json::Value>>(&result_json) {
        // Expect an array of results. Split this up and save eacn one as a JSON blob.
        for result in result_array.iter() {
            match serde_json::to_string(result) {
                Ok(result_json) => results.push(RunResult {
                    event_id,
                    handler_id: task_spec.handler_id,
                    output: Some(result_json),
                    error: None,
                }),
                Err(e) => {
                    log::error!(
                        "Failed to serialize output of task_spec{}: {:?}",
                        task_spec.handler_id,
                        e,
                    );
                    report_error(
                        task_spec,
                        event_id,
                        results,
                        String::from("Failed to parse result from function."),
                    );
                }
            }
        }
    } else {
        report_error(
            task_spec,
event_id,
             results,
            String::from("Failed to parse result from function. Did you return objects that can't be represented in JSON?"),
        );
    }
}

/// Push an error message to the results.
fn report_error(
    task_spec: &HandlerSpec,
    event_id: i64,
    results: &mut Vec<RunResult>,
    message: String,
) {
    results.push(RunResult {
        event_id: event_id,
        handler_id: task_spec.handler_id,
        output: None,
        error: Some(String::from(message)),
    });
}

/// From a Context in which a script has already been loaded and executed, leaving a function named 'f'.
/// Retrieve that function and return it.
/// Returns the function as a Value and cast to a Function, as required by the V8 function invocation API.
/// A little strange, but lets us keep the separation of concerns, and handle both "does f exist" and "is f a function".
fn get_f_function<'s>(
    task_spec: &HandlerSpec,
    results: &mut Vec<RunResult>,
    task_scope: &mut HandleScope<'s>,
    task_proxy: Local<'s, Object>,
) -> Option<(Local<'s, Function>, Local<'s, v8::Value>)> {
    // Now we can look for the function that was registered.
    let function_key = v8::String::new(task_scope, "f").unwrap();

    if let Some(query_function) = task_proxy.get(task_scope, function_key.into()) {
        if !query_function.is_function() {
            report_error(            task_spec,
-1,
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
            -1,
            results,
            String::from("Didn't find named function."),
        );
        None
    }
}

fn load_script(
    task_spec: &HandlerSpec,
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
                            -1,
                            results,
                            format!("Failed to load the function. Exception: {}", message),
                        );
                        false
                    } else {
                        report_error(
                            task_spec,
                            -1,
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
            report_error(
                task_spec,
                -1,
                results,
                String::from("Failed to compile code."),
            );
            false
        }
    } else {
        report_error(task_spec, -1, results, String::from("Failed to load code."));
        false
    }
}

/// Marshal a JSON input a parsed value in the context.
/// Return the handle.
fn marshal_task_input<'s>(scope: &mut HandleScope<'s>, json: &str) -> Local<'s, v8::Value> {
    let marshalled_json_input = v8::String::new(scope, &json).unwrap();
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
