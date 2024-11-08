use std::collections::HashSet;

use v8::{Platform, SharedRef, Value, V8};

pub(crate) struct GlobalContext {}

impl GlobalContext {
    pub(crate) fn new() -> GlobalContext {
        let platform = v8::new_default_platform(0, false).make_shared();
        V8::initialize_platform(platform);
        V8::initialize();

        GlobalContext {}
    }
}

/// A task to be run. Identified by user ID, task ID, and the JavaScript code to execute.
#[derive(Debug)]
pub(crate) struct TaskSpec {
    pub(crate) user_id: u64,
    pub(crate) id: u64,
    pub(crate) code: String,
}

#[derive(Debug)]
pub(crate) struct Input {
    pub(crate) data: String,
}

#[derive(Debug)]
pub(crate) struct Result {
    pub(crate) task_id: u64,
    pub(crate) output: Option<String>,
    pub(crate) error: Option<String>,
}

use itertools::Itertools;

/// Run all tasks against all inputs.
/// Create an isolated environment for each distinct user.
pub(crate) fn run_all(tasks: &[TaskSpec], inputs: &[Input]) -> Vec<Result> {
    let by_user_id = tasks.iter().map(|t| (t.user_id, t)).into_group_map();

    let mut results: Vec<Result> = vec![];

    for (user_id, task_specs) in by_user_id.iter() {
        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut v8::HandleScope::new(isolate);

        for task_spec in task_specs.iter() {
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);

            if let Some(code) = v8::String::new(scope, &task_spec.code) {
                if let Some(script) = v8::Script::compile(scope, code, None) {
                    for input in inputs {
                        let g = context.global(scope);
                        let k = v8::String::new(scope, "KEY").unwrap();
                        let v = v8::String::new(scope, &input.data).unwrap();

                        g.set(scope, k.into(), v.into());

                        if let Some(result) = script.run(scope) {
                            let result_str = result.to_rust_string_lossy(scope);

                            results.push(Result {
                                task_id: task_spec.id,
                                output: Some(result_str),
                                error: None,
                            })
                        } else {
                            results.push(Result {
                                task_id: task_spec.id,
                                output: None,
                                error: Some(String::from("Failed to run code.")),
                            });
                        }
                    }
                } else {
                    results.push(Result {
                        task_id: task_spec.id,
                        output: None,
                        error: Some(String::from("Failed to compile code.")),
                    });
                }
            } else {
                results.push(Result {
                    task_id: task_spec.id,
                    output: None,
                    error: Some(String::from("Failed to load code.")),
                });
            }
        }
    }

    results
}
