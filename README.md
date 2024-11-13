# Pardalotus Metabeak

The code that powers the Pardalotus API.

Work in progress, pre-release.

# Development

Prepare DB:
```sh
(cd etc; ./reset_db.sh)^
```

Load sample events and handlers:

```sh
RUST_BACKTRACE=1 cargo run -- --load-events samples/events --load-handlers samples/handlers --execute-one
```

Run the message pump.

```sh
RUST_BACKTRACE=1 cargo run -- --execute-one
```

Combined:

```sh
RUST_BACKTRACE=1 cargo run -- --load-events samples/events --load-handlers samples/handlers --execute-one
```





# License

This code is MIT licensed. Copyright 2024 Joe Wass
