# Operation

A PostgreSQL database is needed. First time, load the `etc/schema.sql` file.
Currently there are no database migrations. This will be added as needed.

Help:
```sh
./metabeak -h
```

To manually load handler functions from disk. Each file in the directory should be a JavaScript script.

```sh
./metabeak --load-handlers samples/handlers
```

To manually load Events from disk. Each file should be a JSON file containing an array of Events.

```sh
./metabeak --load-events samples/events
```
