# Developing Metabeak

Setup local PostgreSQL. This will destroy and create the database and user.

```sh
psql < etc/recreate_db.sql
```

Run, loading sample files.

```sh
cargo run -- --load examples
```
