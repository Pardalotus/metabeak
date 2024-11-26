# Pardalotus Metabeak

The code that powers the Pardalotus API.

Work in progress, pre-release.

# Development

Drop and create DB:
```sh
(cd etc; ./reset_db.sh)
```

Load sample events and handlers:

```sh
cargo run -- --load-events samples/events --load-handlers samples/handlers --execute-one
```

Fetch some metadata assertions from Crossref:

```sh
cargo run -- --fetch-crossref
```

Extract events from metadata assertions:

```sh
cargo run -- --extract
```

Run handler functions for events

```sh
cargo run -- --execute
```

Run API

```sh
cargo run -- --api
```

These flags can all be combined, except `--execute`, which are `--api` are blocking and mutually exclusive.

## API

To upload a function:

```
$ curl -F data=@./samples/handlers/hello.js localhost:6464/functions
```

```json
{
  "status": "created",
  "data": {
    "id": 44,
    "code": "var f = function (arg) {\n  return [\"Hello\", \"World??\", arg];\n};\n",
    "status": "Enabled"
  }
}
```

 - Browse functions at <http://localhost:6464/functions>
 - View function info at <http://localhost:6464/functions/44>
 - View code for a function at <http://localhost:6464/functions/44/code.json>
 - View results <http://localhost:6464/functions/44/results>
 - View debug results <http://localhost:6464/functions/44/debug>

When a `cursor` value is returned, pass it with `?cursor=` to get the next page. These cursors do not timeout, although the data may.

# License

Copyright 2024 Joe Wass, Pardalotus Technology. This code is Apache 2 licensed, see the LICENSE.txt and NOTICE files.
