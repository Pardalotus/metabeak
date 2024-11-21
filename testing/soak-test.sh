set -e
cargo build

target/debug/metabeak --load-handlers samples/handlers/

truncate -s 0 testing/soak-test.log

while true
do
    target/debug/metabeak --fetch-crossref >> testing/soak-testing.log 2>&1
    target/debug/metabeak --extract >> testing/soak-testing.log 2>&1
    target/debug/metabeak --execute >> testing/soak-testing.log 2>&1
    sleep 60
    echo "Tick"
done
