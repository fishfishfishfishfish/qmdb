#!/bin/bash

cargo build --release

entries_counts=(500 1000 2000 3000 4000 5000)
tps_blocks=100

for entries_count in "${entries_counts[@]}"; do
    set -x
    rm -rf ${PWD}/data
    ./target/release/update_metas --db-dir ${PWD}/data --tps-blocks ${tps_blocks} --entry-count ${entries_count} --ops-per-block ${entries_count} --output-filename "results${entries_count}entries.csv"  > test${entries_count}entries.log 2>error${entries_count}entries.log
    set +x
done