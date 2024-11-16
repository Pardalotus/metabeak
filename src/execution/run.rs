//! Run functions in V8.
//! For each function, spin up a V8 environment and execute the function.

use v8::{Context, Function, HandleScope, Local, Object, V8};

use crate::execution::model::Global;

use super::model::{Event, HandlerSpec, RunResult};

/// Initialize the V8 environment.
pub(crate) fn init() {
    let platform = v8::new_default_platform(0, false).make_shared();
    V8::initialize_platform(platform);
    V8::initialize();
}

/// Given the output of a handler function run, parse it and append the result to the results list.
fn report_result_success(
    handler_spec: &HandlerSpec,
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
            handler_spec,
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
                    handler_id: handler_spec.handler_id,
                    output: Some(result_json),
                    error: None,
                }),
                Err(e) => {
                    log::error!(
                        "Failed to serialize output of handler_spec{}: {:?}",
                        handler_spec.handler_id,
                        e,
                    );
                    report_error(
                        handler_spec,
                        event_id,
                        results,
                        String::from("Failed to parse result from function."),
                    );
                }
            }
        }
    } else {
        report_error(
            handler_spec,
            event_id,
             results,
            String::from("Failed to parse result from function. Check that you returned an array of results that can be represented in JSON."),
        );
    }
}

/// Push an error message to the results.
fn report_error(
    handler_spec: &HandlerSpec,
    event_id: i64,
    results: &mut Vec<RunResult>,
    message: String,
) {
    results.push(RunResult {
        event_id,
        handler_id: handler_spec.handler_id,
        output: None,
        error: Some(message),
    });
}

/// From a Context in which a script has already been loaded and executed, leaving a function named 'f'.
/// Retrieve that function and return it.
/// Returns the function as a Value and cast to a Function, as required by the V8 function invocation API.
/// A little strange, but lets us keep the separation of concerns, and handle both "does f exist" and "is f a function".
fn get_f_function<'s>(
    handler_spec: &HandlerSpec,
    results: &mut Vec<RunResult>,
    task_scope: &mut HandleScope<'s>,
    task_proxy: Local<'s, Object>,
) -> Option<(Local<'s, Function>, Local<'s, v8::Value>)> {
    // Now we can look for the function that was registered.
    let function_key = v8::String::new(task_scope, "f").unwrap();

    if let Some(query_function) = task_proxy.get(task_scope, function_key.into()) {
        if !query_function.is_function() {
            report_error(            handler_spec,
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
            handler_spec,
            -1,
            results,
            String::from("Didn't find named function."),
        );
        None
    }
}

/// Load the script from the HandlerSpec into the given V8 Context.
/// Return success, log errors to results vec.
fn load_script(
    handler_spec: &HandlerSpec,
    results: &mut Vec<RunResult>,
    task_scope: &mut HandleScope<'_, Context>,
) -> bool {
    if let Some(code) = v8::String::new(task_scope, &handler_spec.code) {
        if let Some(script) = v8::Script::compile(task_scope, code, None) {
            let mut try_catch_scope = v8::TryCatch::new(task_scope);

            let run = script.run(&mut try_catch_scope);

            match run {
                None => {
                    if let Some(ex) = try_catch_scope.exception() {
                        let message = ex.to_rust_string_lossy(&mut try_catch_scope);
                        report_error(
                            handler_spec,
                            -1,
                            results,
                            format!("Failed to load the function. Exception: {}", message),
                        );
                        false
                    } else {
                        report_error(
                            handler_spec,
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
                handler_spec,
                -1,
                results,
                String::from("Failed to compile code."),
            );
            false
        }
    } else {
        report_error(
            handler_spec,
            -1,
            results,
            String::from("Failed to load code."),
        );
        false
    }
}

/// Marshal a JSON input a parsed value in the context.
/// Return the handle.
fn marshal_task_input<'s>(scope: &mut HandleScope<'s>, json: &str) -> Local<'s, v8::Value> {
    let marshalled_json_input = v8::String::new(scope, json).unwrap();
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

/// Run all tasks against all inputs.
/// Create an isolated environment for each distinct user.
pub(crate) fn run_all(handlers: &[HandlerSpec], events: &[Event]) -> Vec<RunResult> {
    log::info!(
        "Run {} tasks against {} inputs",
        handlers.len(),
        events.len()
    );

    let mut results: Vec<RunResult> = vec![];

    // Representation of the global 'environment' variable provided to all function invocations.
    let environment_json = Global::build().json();

    // Build the full JSON for each, including hydrating identifiers etc.
    let hydrated_events: Vec<(&Event, String)> = events
        .iter()
        .filter_map(|event| match event.to_json_value() {
            Some(json) => Some((event, json)),
            None => None,
        })
        .collect();

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
                get_f_function(handler_spec, &mut results, task_scope, task_proxy)
            {
                // Execute f for each input.
                for (event, json) in hydrated_events.iter() {
                    let input_handle = marshal_task_input(task_scope, &json);

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
                                    event.event_id,
                                    &mut results,
                                    format!("Failed to run the function. Exception: {}", message),
                                );
                            } else {
                                report_error(
                                    handler_spec,
                                    event.event_id,
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
                                event.event_id,
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
