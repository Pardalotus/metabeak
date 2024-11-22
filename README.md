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

These flags can all be combined.

# License

Copyright 2024 Joe Wass, Pardalotus Technology. This code Apache 2 licensed, see the LICENSE.txt and NOTICE files.
