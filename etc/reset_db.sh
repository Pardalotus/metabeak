set -e
psql < recreate_db.sql
psql postgres://metabeak:metabeak@localhost/metabeak < schema.sql
