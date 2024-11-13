# Decision Register for Pardalotus Metabeak

Design decisions, to explain how the code was built. Intended for developers on the codebase, not users of the service.

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

## 2: Different V8 context per handler function.

## 3: Maintain V8 context between handler function executions

## 4: Simple function

Options:

1. Named function, e.g. `function f() { return ["result"] };`
2. Expression e.g. `["result"]`
3. JS Module ([e.g.](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Modules).

### Decision

1, named function. Simple enough for quick usage. Easy to add optional values, e.g. description, as variables.

## 5: Load functions prior to invocation

Options:

1. Load, keep many VMs going
2. Load prior to execution

Factors

1. RAM consumption. Linear CPU vs linear memory consumption.
2. Throughput. But can scale with batch size of data.

3. Allow for splitting up invocation to scale out in future. Not reliant on having everything loaded.
4. SQL overhead isn't much more than the data.

## 6. Tokio async

default choice

## 7. Function is subscribed to all events

Function can easily check for source.
Easier to understand how function executes, don't need to know config.
Function can receive new sources without changing

## 8. New function ID when code changes

rather than an identifier that can be updated


## 9: Signed integers for ids

PostgreSQL has only signed ints. No point storing unsigned ones. Less conversion overhead and scope for errors.

## 10: Naming of concepts

- "Source" is the name for a place data comes from. E.g. 'crossref', 'datacite', 'wikipedia' etc. It's about provenance not ownership. Crossref could assert metadata about a Datacite DOI for example.
- "Entity" is something with an identifier, e.g. a scholarly work with a DOI.
- "Metadata" is an assertion of some scholarly metadata.
- "Event Analyzer" is code that analyzes a metadata assertion.
- "Event" is the input to a Handler. It is derived from a Metadata Assertion from a Source by an Event Analyzer.
- "Handler Function" is user-supplied code that consumes an Event. Alternative words would be 'function', which makes the code a bit ambiguous. The word 'function' might be used in user documentation though.

Users only really need to know about Handler and Events.

## 11: Minimal structure of an Event

JSON with the following fields:

 - `source` - where the original data came from
 - `analyzer` - who produced the Event
 - `type` - type of Event, from a stated vocabulary
 - `subject` - PID of the subject of the event

Each type will have a set of fields.

## DR-0012: Represent Event objects as serde JSON or String

All Events are JSON. On ingestion they must validate as JSON, and some data is
normalised out. On execution they are straight to the handler, but some
manpulation is done first.

### Options:

1. `Event` struct has a JSON as a serde_json value.
2. `Event` struct has a String field.

### Factors

1. Memory overhead
2. CPU overhead

The following issues highlight the overhead of serde Value:
 - <https://github.com/serde-rs/json/issues/635>
 - <https://www.reddit.com/r/rust/comments/pa3jtu/poor_performance_and_high_memory_usage/>
 - <https://stackoverflow.com/questions/76454260/rust-serde-get-runtime-heap-size-of-vecserde-jsonvalue>

### DR-0013: Represent whole Event struct in code vs JSON Schema

Make Event mostly opaque to the code. Although it would more efficient to store it this way it would introduce tech debt. Better to make validation optional and/or mark deprecated parts of the schema than keep old schemas around in the enum definition.

### DR-0014: Expiry window

The focus of the API is live data streaming, not historical data.

Some time period will be published for the expiry of data. For example, it may
be retained for 1 day, 1 week, etc.

### DR-0015: Foreign Key Integrity

Different entities link together with foreign keys.

However, the focus of the API is live data streaming, and data is all ephemeral.
It should be possible to expire data at different rates. E.g. Events are
probably going to be expired before the data they are based on.

But it should be possible to delete data in a way that's compatible with the
public expiry window (DR-0014) but that might violate foreign key integrity.

All foreign keys should therefore be treated as weak references, not strong
ones. All code that fetch foreign keys, and data models, should deal with this.

## DR-0016 Tracking execution

Per DR-0007 every Handler runs for every Event.

In the first iteration there's no multiprocessing of the executor. This means
that the database doesn't need to track individual executions.

Therefore the event_queue SQL table tracks simply 'was this event passed to
all handler functions'.

It may be necessary to revisit this if there's enough need to scale out.

Tuples on the `event` table will be comparatively large, as they contain a blob
of JSON. Setting a flag on this would lead to bloat from dead tuples. The
`event_queue` table is therefore separate to the `event` table.
