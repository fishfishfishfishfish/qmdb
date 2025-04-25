#!/bin/bash
export PATH=$PATH:/home/${USER}/.cargo/bin
cargo clean
cargo build --release

entries_counts=(80)
tps_blocks=20
ops_per_block=20 # batch size
key_size=32
value_size=1024

result_dir="micro_benchmark_results"
mkdir -p ${result_dir}
rm -rf ${result_dir}/*

for entries_count in "${entries_counts[@]}"; do
    set -x
    rm -rf ${PWD}/data
    ./target/release/micro_benchmark --db-dir ${PWD}/data --tps-blocks ${tps_blocks} --entry-count ${entries_count} --ops-per-block ${ops_per_block} --key-size ${key_size} --val-size ${value_size} --output-filename "${result_dir}/results${entries_count}entries.csv"  > ${result_dir}/test${entries_count}entries.log 2> ${result_dir}/error${entries_count}entries.log
    set +x
done