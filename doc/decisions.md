# Decision Register for Pardalotus Metabeak

## 1: V8 for functions

### Objective: Allow user-supplied queries.

V8's Rust bindings were released late 2024, which was part of the inspiration for this project.

### Options considered

1. Simple filters
2. Custom query language
3. Existing query language
4. User-supplied functions in embedded Python
5. User-supplied functions in embedded JavaScript
7. User-supplied functions in embedded Lua

### Decision

5, embedded JavaScript with V8.

## 2: Different V8 context per task.

## 3: Maintain V8 context between task executions

## 4: Simple function

Options:

1. Named function, e.g. `function f() { return ["result"] };`
2. Expression e.g. `["result"]`
3. JS Module ([e.g.](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Modules).

### Decision

1, named function. Simple enough for quick usage. Easy to add optional values, e.g. description, as variables.
