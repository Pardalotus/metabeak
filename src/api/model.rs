use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{db::handler::HandlerState, execution::model::ExecutionResult};

use super::HandlerSpec;

#[derive(Serialize)]
pub(crate) struct ErrorPage {
    pub(crate) status: String,
    pub(crate) message: String,
}

impl ErrorPage {
    pub(crate) fn new(status: &str, message: &str) -> Self {
        Self {
            status: String::from(status),
            message: String::from(message),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct Function {
    pub(crate) id: i64,
    pub(crate) code: String,
    pub(crate) status: HandlerState,
}

impl From<HandlerSpec> for Function {
    fn from(value: HandlerSpec) -> Self {
        Function {
            id: value.handler_id,
            code: value.code,
            status: match value.status {
                1 => HandlerState::Enabled,
                2 => HandlerState::Disabled,
                _ => HandlerState::Unknown,
            },
        }
    }
}

#[derive(Serialize)]
pub(crate) struct FunctionPage {
    pub(crate) status: String,
    pub(crate) data: Function,
}

impl From<HandlerSpec> for FunctionPage {
    fn from(value: HandlerSpec) -> Self {
        FunctionPage {
            status: String::from("ok"),
            data: Function::from(value),
        }
    }
}

impl From<(HandlerSpec, String)> for FunctionPage {
    fn from((value, status): (HandlerSpec, String)) -> Self {
        FunctionPage {
            status,
            data: Function::from(value),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct FunctionsPage {
    pub(crate) status: String,
    pub(crate) data: Vec<Function>,
}

impl From<Vec<HandlerSpec>> for FunctionsPage {
    fn from(value: Vec<HandlerSpec>) -> Self {
        FunctionsPage {
            status: String::from("ok"),
            data: value.into_iter().map(Function::from).collect(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ResultsPage {
    pub(crate) status: String,
    pub(crate) cursor: i64,
    pub(crate) data: Vec<Value>,
}

impl From<(Vec<Value>, i64)> for ResultsPage {
    fn from((data, cursor): (Vec<Value>, i64)) -> Self {
        ResultsPage {
            status: String::from("ok"),
            data,
            cursor,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ResultQuery {
    pub(crate) cursor: Option<i64>,
}

#[derive(Serialize)]
pub(crate) struct ResultsDebugPage {
    pub(crate) status: String,

    pub(crate) cursor: i64,
    pub(crate) data: Vec<ExecutionResult>,
}

impl From<(Vec<ExecutionResult>, i64)> for ResultsDebugPage {
    fn from((data, cursor): (Vec<ExecutionResult>, i64)) -> Self {
        ResultsDebugPage {
            status: String::from("ok"),
            data,
            cursor,
        }
    }
}
