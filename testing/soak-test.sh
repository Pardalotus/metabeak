set -e

# Build and run loop
# Execute with: systemd-inhibit testing/soak-test.sh

cargo build

target/debug/metabeak --load-handlers samples/handlers/

FILE=testing/`date -Iminutes`-soak.log

while true
do
    echo "Start"
    target/debug/metabeak --fetch-crossref >> $FILE 2>&1
    target/debug/metabeak --extract >> $FILE 2>&1
    target/debug/metabeak --execute >> $FILE 2>&1

    psql metabeak -c "SELECT table_name, (SELECT n_live_tup FROM pg_stat_user_tables WHERE relname = table_name) AS row_count, pg_size_pretty(pg_total_relation_size(quote_ident(table_name))) FROM information_schema.tables WHERE table_schema = 'public' order by row_count desc;"   >> $FILE
    echo "Sleep"
    sleep 900
done
