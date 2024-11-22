//! Run functions in V8.
//! For each function, spin up a V8 environment and execute the function.

use std::{
    sync::{
        mpsc::{self, RecvTimeoutError},
        Once,
    },
    thread,
    time::Duration,
};

use v8::{Context, Function, HandleScope, IsolateHandle, Local, Object, V8};

use crate::execution::model::Global;

use super::model::{Event, HandlerSpec, RunResult};

static V8_INITIALIZED: Once = Once::new();

// Maximum time a JS execution can take.
static EXECUTION_TIMEOUT: Duration = Duration::from_millis(10);

// Maximum time a JS load can take. This takes a while as the environment is set up.
static LOAD_TIMEOUT: Duration = Duration::from_millis(10);

/// Initialize the V8 environment.
/// Guard against re-initialization to make this safe to use, especially calling from tests.
pub(crate) fn init() {
    V8_INITIALIZED.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        V8::initialize_platform(platform);
        V8::initialize();
    })
}

/// Given the output of a handler function run, parse it and append the result to the results list.
fn report_result_output(
    handler_spec: &HandlerSpec,
    event_id: i64,
    results: &mut Vec<RunResult>,
    result: Local<'_, v8::Value>,
    scope: &mut HandleScope<'_, Context>,
) {
    let result_json = v8::json::stringify(scope, result)
        .unwrap()
        .to_rust_string_lossy(scope);

    // If there's no return statement, or JSON serialization fails, 'undefined' is returned.
    // This value itself won't parse as JSON. Handle as a special case.
    if result_json.eq(&"undefined") {
        report_error(
            handler_spec.handler_id,
            event_id,
            results,
            String::from(
                "Function didn't return a JSON-serializable value. Check for a `return` statement.",
            ),
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
                        handler_spec.handler_id,
                        event_id,
                        results,
                        String::from("Failed to parse result from function."),
                    );
                }
            }
        }
    } else {
        report_error(
            handler_spec.handler_id,
            event_id,
             results,
            String::from("Failed to parse result from function. Check that you returned an array of results that can be represented in JSON."),
        );
    }
}

/// Push an error message to the results.
fn report_error(handler_id: i64, event_id: i64, results: &mut Vec<RunResult>, message: String) {
    results.push(RunResult {
        event_id,
        handler_id,
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
            report_error(            handler_spec.handler_id,
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
            handler_spec.handler_id,
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
                            handler_spec.handler_id,
                            -1,
                            results,
                            format!("Failed to load the function. Exception: {}", message),
                        );
                        false
                    } else {
                        report_error(
                            handler_spec.handler_id,
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
                handler_spec.handler_id,
                -1,
                results,
                String::from("Failed to compile code."),
            );
            false
        }
    } else {
        report_error(
            handler_spec.handler_id,
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

    // Run a watchdog thread in the background. It is notified of new isolates are created, along with a timeout value.

    // Messages: start isolate for handler id.
    let (watchdog_send_handler, watchdog_receive_handler) =
        mpsc::channel::<Option<(IsolateHandle, i64, Duration)>>();

    // Messages: handler id was terminated.
    let (watchdog_send_terminated, watchdog_receive_terminated) = mpsc::channel::<i64>();

    // Watchdog thread for all handlers that will run.
    // State machine driven from channel:
    // When a handler is sent, it will wait for another message with a timeout. If none comes, it will terminate that handler.
    // When None is sent, it will wait for a new handler to watch.
    let watchdog_thread = thread::spawn(move || {
        let mut done = false;
        let mut current_isolate: Option<IsolateHandle> = None;
        let mut current_handler_id = -1;
        // Initial value is arbitrary.
        let mut current_duration = EXECUTION_TIMEOUT;
        while !done {
            match watchdog_receive_handler.recv_timeout(current_duration) {
                // If one was sent, store it to set the timeout. If None was sent, store that to reset the timeout.
                Ok(maybe_isolate) => {
                    if let Some((isolate, handler_id, duration)) = maybe_isolate {
                        current_isolate = Some(isolate);
                        current_handler_id = handler_id;
                        current_duration = duration;
                    } else {
                        current_isolate = None;
                        current_handler_id = -1;
                    }
                }
                Err(error) => match error {
                    RecvTimeoutError::Disconnected => done = true,
                    RecvTimeoutError::Timeout => {
                        if let Some(isolate) = current_isolate {
                            log::info!(
                                "Terminate handler id {} exceeded {:?}",
                                current_handler_id,
                                current_duration
                            );
                            watchdog_send_terminated.send(current_handler_id).unwrap();
                            isolate.terminate_execution();
                            current_isolate = None;
                            current_handler_id = -1;
                        }
                    }
                },
            }
        }
    });

    let mut results: Vec<RunResult> = vec![];

    // Representation of the global 'environment' variable provided to all function invocations.
    let environment_json = Global::build().json();

    // Build the full JSON for each, including hydrating identifiers etc.
    let hydrated_events: Vec<(&Event, String)> = events
        .iter()
        .filter_map(|event| event.to_json_value().map(|json| (event, json)))
        .collect();

    // Isolated environment for each task, re-used for all input data.
    for handler_spec in handlers.iter() {
        log::debug!("Running task id {}", handler_spec.handler_id);

        let isolate = &mut v8::Isolate::new(Default::default());

        // Handle that can be sent to watchdog thread.
        let watchdog_handle = isolate.thread_safe_handle();

        let handle_scope = &mut v8::HandleScope::new(isolate);

        // Each task associated with the user.
        let task_context = v8::Context::new(handle_scope, Default::default());
        let task_scope = &mut v8::ContextScope::new(handle_scope, task_context);
        let task_proxy = task_context.global(task_scope);

        // Set the global 'environment' variable.
        set_variable_from_json(task_scope, task_proxy, "environment", &environment_json);

        // Start the timer for the watchdog.
        // Load can take a few milliseconds.
        watchdog_send_handler
            .send(Some((
                watchdog_handle.clone(),
                handler_spec.handler_id,
                LOAD_TIMEOUT,
            )))
            .unwrap();

        // Load the script from the task spec and execute it.
        // The script should define a function called 'f', which we'll retrieve from the scope.
        // This means we don't need to retain a direct handle to the script itself once it's executed.
        // On failure, log exception message to results.
        let ok: bool = load_script(handler_spec, &mut results, task_scope);

        watchdog_send_handler.send(None).unwrap();

        // Now retrieve the function from the context.
        if ok {
            if let Some((function_as_f, function_as_v)) =
                get_f_function(handler_spec, &mut results, task_scope, task_proxy)
            {
                // Execute f for each input.
                // Function execution should be much quicker than loading.
                for (event, json) in hydrated_events.iter() {
                    let input_handle = marshal_task_input(task_scope, json);

                    // Run in a TryCatch so we can retrieve error messages.
                    let mut try_catch_scope = v8::TryCatch::new(task_scope);

                    // Start the watchdog timer for this isolate.
                    // We will terminate the whole isolate, not this function execution, but that's proportionate for a misbehaving function.
                    watchdog_send_handler
                        .send(Some((
                            watchdog_handle.clone(),
                            handler_spec.handler_id,
                            EXECUTION_TIMEOUT,
                        )))
                        .unwrap();

                    let run =
                        function_as_f.call(&mut try_catch_scope, function_as_v, &[input_handle]);

                    // Reset watchdog if it terminated normally.
                    watchdog_send_handler.send(None).unwrap();

                    match run {
                        None => {
                            // Run failed. Try to report the exception.
                            if let Some(ex) = try_catch_scope.exception() {
                                let message = ex.to_rust_string_lossy(&mut try_catch_scope);
                                report_error(
                                    handler_spec.handler_id,
                                    event.event_id,
                                    &mut results,
                                    format!("Failed to run the function. Exception: {}", message),
                                );
                            } else {
                                report_error(
                                    handler_spec.handler_id,
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
                            report_result_output(
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

        // Poll  for any terminated handlers and report.
        report_terminated(&watchdog_receive_terminated, &mut results);
    }

    drop(watchdog_send_handler);

    // Watchdog thread must exit or it'll keep ticking away, which would cause a memory leak.
    // If it doesn't terminate almost immediately that's a bug, and it's better to hang or panic.
    log::debug!("Wait for watchdog...");
    watchdog_thread.join().unwrap();
    log::debug!("Watchdog stopped.");

    results
}

/// Poll from 'terminated handler' channel and report an error message.
fn report_terminated(terminated_chan: &mpsc::Receiver<i64>, results: &mut Vec<RunResult>) {
    // Read until we got all messages, not until it closed.
    for handler_id in terminated_chan.try_iter() {
        report_error(
            handler_id,
            -1,
            results,
            String::from("Handler function took too long to run and was terminated."),
        );
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the V8 runtime. These are all run serially to avoid concurrency issues, as V8 uses a global object.

    use scholarly_identifiers::identifiers::Identifier;
    use serial_test::serial;

    use super::*;

    fn init_tests() {
        init();
    }

    /**
     * Happy paths.
     */

    /// When multiple results are returned from a function each should have a result.
    #[test]
    #[serial]
    fn multiple_results() {
        init_tests();

        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from("function f(args) { return [{\"result\": \"one\"}, {\"result\": \"two\"}, {\"result\": \"three\"}]; }"),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 4321,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        assert_eq!(
            results,
            vec![
                RunResult {
                    handler_id: 1234,
                    event_id: 4321,
                    output: Some(String::from("{\"result\":\"one\"}")),
                    error: None
                },
                RunResult {
                    handler_id: 1234,
                    event_id: 4321,
                    output: Some(String::from("{\"result\":\"two\"}")),
                    error: None
                },
                RunResult {
                    handler_id: 1234,
                    event_id: 4321,
                    output: Some(String::from("{\"result\":\"three\"}")),
                    error: None
                }
            ]
        );
    }

    /// When an empty array is returned, zero results should be collected.
    #[test]
    #[serial]
    fn empty_array_ok() {
        init_tests();

        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from("function f(args) { return []; }"),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 4321,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        assert_eq!(results, vec![], "No results expected.");
    }

    /// Whole hydrated Event is passed into the function.
    #[test]
    #[serial]
    fn hydrated_input_event() {
        init_tests();

        // Function that echoes input so we can test it.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from("function f(args) { return [args]; }"),
        }];

        // Event using an Identifier.
        // The JSON contains {"hello": "world"} and the other fields should be hydrated into it when supplied to the handler function.
        let events: Vec<Event> = vec![Event {
            event_id: 4321,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: Some(Identifier::parse("https://doi.org/10.5555/12345678")),
            object_id: Some(Identifier::parse("https://doi.org/10.5555/242424x")),
            json: String::from("{\"hello\": \"world\"}"),
        }];

        let results = run_all(&handlers, &events);

        let returned_json: serde_json::Value =
            serde_json::from_str(&results.first().unwrap().output.clone().unwrap().clone())
                .unwrap();

        assert_eq!(
            returned_json.get("source").unwrap(),
            &serde_json::Value::String(String::from("test")),
            "Source value should be hydrated."
        );

        assert_eq!(
            returned_json.get("hello").unwrap(),
            &serde_json::Value::String(String::from("world")),
            "Original JSON fields expected."
        );

        assert_eq!(
            returned_json.get("analyzer").unwrap(),
            &serde_json::Value::String(String::from("test")),
            "Analyzer value should be hydrated."
        );

        assert_eq!(
            returned_json.get("subject_id").unwrap(),
            &serde_json::Value::String(String::from("10.5555/12345678")),
            "subject_id value should be hydrated."
        );

        assert_eq!(
            returned_json.get("subject_id_type").unwrap(),
            &serde_json::Value::String(String::from("doi")),
            "subject_id_type value should be hydrated."
        );

        assert_eq!(
            returned_json.get("subject_id_uri").unwrap(),
            &serde_json::Value::String(String::from("https://doi.org/10.5555/12345678")),
            "subject_id_uri value should be hydrated."
        );

        assert_eq!(
            returned_json.get("object_id").unwrap(),
            &serde_json::Value::String(String::from("10.5555/242424x")),
            "object_id value should be hydrated."
        );

        assert_eq!(
            returned_json.get("object_id_type").unwrap(),
            &serde_json::Value::String(String::from("doi")),
            "object_id_type value should be hydrated."
        );

        assert_eq!(
            returned_json.get("object_id_uri").unwrap(),
            &serde_json::Value::String(String::from("https://doi.org/10.5555/242424x")),
            "object_id_uri value should be hydrated."
        );
    }

    /// All handlers should run against all Events.
    #[test]
    #[serial]
    fn all_handlers_all_events() {
        init_tests();

        // Three handlers. Each with a different ID each with a distinctive output.
        let handlers: Vec<HandlerSpec> = vec![
            HandlerSpec {
                handler_id: 1,
                code: String::from("function f(args) { return [args.x + '-one']; }"),
            },
            HandlerSpec {
                handler_id: 2,
                code: String::from("function f(args) { return [args.x + '-two']; }"),
            },
            HandlerSpec {
                handler_id: 3,
                code: String::from("function f(args) { return [args.x + '-three']; }"),
            },
        ];

        // Three events, each with a distinctive input value.
        let events: Vec<Event> = vec![
            Event {
                event_id: 1,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{\"x\": \"one\"}"),
            },
            Event {
                event_id: 2,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{\"x\": \"two\"}"),
            },
            Event {
                event_id: 3,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{\"x\": \"three\"}"),
            },
        ];

        let results = run_all(&handlers, &events);

        assert_eq!(
            results,
            vec![
                RunResult {
                    handler_id: 1,
                    event_id: 1,
                    output: Some(String::from("\"one-one\"")),
                    error: None
                },
                RunResult {
                    handler_id: 1,
                    event_id: 2,
                    output: Some(String::from("\"two-one\"")),
                    error: None
                },
                RunResult {
                    handler_id: 1,
                    event_id: 3,
                    output: Some(String::from("\"three-one\"")),
                    error: None
                },
                RunResult {
                    handler_id: 2,
                    event_id: 1,
                    output: Some(String::from("\"one-two\"")),
                    error: None
                },
                RunResult {
                    handler_id: 2,
                    event_id: 2,
                    output: Some(String::from("\"two-two\"")),
                    error: None
                },
                RunResult {
                    handler_id: 2,
                    event_id: 3,
                    output: Some(String::from("\"three-two\"")),
                    error: None
                },
                RunResult {
                    handler_id: 3,
                    event_id: 1,
                    output: Some(String::from("\"one-three\"")),
                    error: None
                },
                RunResult {
                    handler_id: 3,
                    event_id: 2,
                    output: Some(String::from("\"two-three\"")),
                    error: None
                },
                RunResult {
                    handler_id: 3,
                    event_id: 3,
                    output: Some(String::from("\"three-three\"")),
                    error: None
                }
            ]
        );
    }

    //
    // Error cases.
    //
    // Problems can occur when loading the function and when executing it into
    // the V8 Isolate. The [run_all] function encapsulates both. Tests named
    // _run refer to running the handler, tests named _load refer to loading the
    // handler.

    /// When a non-JSON serializable result is returned, an appropriate error result is returned.
    #[test]
    #[serial]
    fn non_serializable_error_run() {
        init_tests();

        // Handler that returns a function, which isn't JSON-serializable.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from("function x() {}; function f(args) { return x; }"),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 4321,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        assert_contains(
            4321,
            1234,
            "Function didn't return a JSON-serializable",
            &results,
        );
    }

    /// When nothing is returned, an appropriate error result is returned.
    #[test]
    #[serial]
    fn no_return_error_run() {
        init_tests();

        // Handler that doesn't return anything.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from("{}; function f(args) { }"),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 4321,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        let ok = results
            .first()
            .unwrap()
            .error
            .clone()
            .unwrap()
            .contains("Function didn't return a JSON-serializable");

        assert!(ok, "Expected error message");
    }

    /// Stackoverflow on run gives an error.
    #[test]
    #[serial]
    fn stack_overflow_run() {
        init_tests();

        // Function that deliberately stack-overflows on run.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from(
                "function x(i) { return x(i+1); } function f(args) { return x(1); }",
            ),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 4321,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        // We hit the timeout before we exhaust the stack. But stack overflow is also handled by V8.
        assert_contains(-1, 1234, "took too long to run", &results);
    }

    /// Stackoverflow on load gives an error.
    #[test]
    #[serial]
    fn stack_overflow_load() {
        init_tests();

        // Function that deliberately stack-overflows on load.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from(
                "function x(i) { return x(i+1); }; x(1); function f(args) { return [1] }",
            ),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 4321,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        // Because the load timeout is more liberal, we hit stack overflow fault before timeout.
        assert_contains(-1, 1234, "Maximum call stack size exceeded", &results);
    }

    /// A handler that is slow to load is terminated and not loaded.
    /// It is not run for any event inputs.
    #[test]
    #[serial]
    fn slow_handler_load() {
        init_tests();

        // Function that never ends on initialization.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from(
                " let r = 0; while(true) {r += 1}; function f(args) {
                    return [1];
                }",
            ),
        }];

        // Send 2 events. Neither should be executed.
        let events: Vec<Event> = vec![
            Event {
                event_id: 4321,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
            Event {
                event_id: 1234,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
        ];

        let results = run_all(&handlers, &events);

        let error_results = results.iter().filter(|r| {
            r.handler_id == 1234
                && r.event_id == -1
                && r.error.clone().unwrap().contains("too long")
        });
        assert!(
            error_results.count() > 0,
            "Expected at least one error message about timeout."
        );

        assert_contains(-1, 1234, "Failed to load the function", &results);
    }

    /// A handler that loaded OK but is slow to run is terminated.
    /// This example works fine the first time but takes too long the second time.
    #[test]
    #[serial]
    fn slow_handler_run() {
        init_tests();

        // Function that executes once and returns its input. Second time it doesn't terminate.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from(
                "let c = 0;
                function f(args) {
                    let r = 0;
                    if (c > 0) {
                        while(true) {r += 1};
                    }
                    c += 1;

                    return [args];
                }",
            ),
        }];

        // Send 2 events. Neither should be executed.
        let events: Vec<Event> = vec![
            Event {
                event_id: 1111,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
            Event {
                event_id: 2222,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
            Event {
                event_id: 3333,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
        ];

        let results = run_all(&handlers, &events);

        assert_eq!(
            (
                results.first().unwrap().event_id,
                results.first().unwrap().handler_id,
            ),
            (1111, 1234),
            "Expected first event to be processed."
        );

        assert_eq!(
            results.first().unwrap().error,
            None,
            "Expected first event to be processed without error."
        );

        // Expect a message for the handler, not linked to the Event.
        // Don't enforce a spec about many errors are reported, just that there was at least one.
        assert_contains(-1, 1234, "too long", &results);
    }

    /// Both the loading and the function take too long to execute. In this case
    /// the function will never be loaded or executed, but here's a test case to
    /// illustrate what happens.
    #[test]
    #[serial]
    fn slow_handler_load_run() {
        init_tests();

        // Function with infinite loop on load and theoretically execution.
        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from(
                "let r = 0;
                while(true) {r += 1};

                function f(args) {
                    while(true) {r += 1};
                    return [args];
                }",
            ),
        }];

        let events: Vec<Event> = vec![
            Event {
                event_id: 1111,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
            Event {
                event_id: 2222,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
            Event {
                event_id: 3333,
                analyzer: crate::db::source::EventAnalyzerId::Test,
                source: crate::db::source::MetadataSourceId::Test,
                subject_id: None,
                object_id: None,
                json: String::from("{}"),
            },
        ];

        let results = run_all(&handlers, &events);

        // Expect a message for the handler, not linked to the Event.
        // Don't enforce a spec about many errors are reported, just that there was at least one.
        assert_contains(-1, 1234, "too long", &results);
    }

    // Language features.

    /// The Deno variable shouldn't be accessible.
    #[test]
    #[serial]
    fn prohibited_deno() {
        init_tests();

        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from(
                "Deno.serve((_req) => {
                  return new Response('Hello, World!');
                });",
            ),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 1111,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        assert_contains(-1, 1234, "Deno is not defined", &results);
    }

    /// The JSON functions should be available.
    /// Not much use, but who knows.
    #[test]
    #[serial]
    fn son_deno() {
        init_tests();

        let handlers: Vec<HandlerSpec> = vec![HandlerSpec {
            handler_id: 1234,
            code: String::from("function f() {return [JSON.stringify([1,2,3])] }"),
        }];

        let events: Vec<Event> = vec![Event {
            event_id: 1111,
            analyzer: crate::db::source::EventAnalyzerId::Test,
            source: crate::db::source::MetadataSourceId::Test,
            subject_id: None,
            object_id: None,
            json: String::from("{}"),
        }];

        let results = run_all(&handlers, &events);

        assert_eq!(
            results,
            vec![RunResult {
                handler_id: 1234,
                event_id: 1111,
                output: Some(String::from("\"[1,2,3]\"")),
                error: None
            }]
        );
    }

    //
    // Util
    //

    fn assert_contains(event_id: i64, handler_id: i64, text: &str, results: &[RunResult]) {
        let error_results = results.iter().filter(|r| {
            r.handler_id == handler_id
                && r.event_id == event_id
                && r.error.clone().unwrap().contains(text)
        });
        assert!(
            error_results.count() > 0,
            "Expected to match at least one result for {}, {}, '{}' in results: {:?}.",
            event_id,
            handler_id,
            text,
            results
        );
    }
}
