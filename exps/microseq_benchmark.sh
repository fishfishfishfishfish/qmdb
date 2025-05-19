#!/bin/bash
export PATH=$PATH:/home/${USER}/.cargo/bin
cd ../
cargo clean
cargo build --release
cd exps/

# entries_counts=(80)
# tps_blocks=20
# ops_per_block=20 # batch size
# key_size=32
# value_size=1024
# entries_counts=(1080000)
# batch_sizes=(3000)
entries_counts=(1000000)
# batch_sizes=(1000)
batch_sizes=(500 1000 2000 4000 5000)
tps_blocks=20
key_size=32
value_sizes=(256 512 1024 2048)

result_dir="${PWD}/results_qmdb/microseq_benchmark"
mkdir -p ${result_dir}
rm -rf ${result_dir}/*

for n_acc in "${entries_counts[@]}"; do
    for ops_per_block in "${batch_sizes[@]}"; do
        for value_size in "${value_sizes[@]}"; do
            set -x
            rm -rf ${PWD}/data
            
            ../target/release/microseq_benchmark --db-dir ${PWD}/data --tps-blocks ${tps_blocks} --entry-count ${n_acc} --ops-per-block ${ops_per_block} --key-size ${key_size} --val-size ${value_size} --output-filename "${result_dir}/e${n_acc}b${ops_per_block}v${value_size}.csv"
            set +x
        done
    done
done

python3 plot_micro_benchmark.py qmdb microseq_benchmark