#!/bin/bash
export PATH=$PATH:/home/${USER}/.cargo/bin
cd ../
# cargo clean
cargo build --release
cd exps/

# 定义测试参数数组
load_account=(20000000)
# load_account=(7000000000)
value_sizes=(1024)


data_path="$PWD/../data/"
result_dir="${PWD}/speed"
mkdir -p $data_path
mkdir -p ${result_dir}
rm -rf ${result_dir}/*

# 运行测试
for n_acc in "${load_account[@]}"; do
    for value_size in "${value_sizes[@]}"; do
        set -x
        # 清理数据文件夹
        rm -rf $data_path/*
        
        result_path="${result_dir}/qmdb_benchmark.json"
        echo $(date "+%Y-%m-%d %H:%M:%S") 
        echo "num account: ${n_acc}, update count:${update_count}, value_size: ${value_size}, key_size: ${key_size}" 
        # 运行测试并提取结果
        # cargo run --bin speed --release -- --db-dir ${data_path} --entry-count $n_acc --ops-per-block 1000000 --tps-blocks 500 --hover-recreate-block 100 --hover-write-block 100 --hover-interval 1000 --output-filename $result_path 
        # ../target/release/speed --db-dir ${data_path} --entry-count $n_acc --ops-per-block 1000000 --tps-blocks 500 --hover-recreate-block 100 --hover-write-block 100 --hover-interval 1000 --output-filename $result_path 
        RUST_BACKTRACE=full ../target/release/speed --db-dir ${data_path} --entry-count $n_acc --ops-per-block 100000 --tps-blocks 5 --hover-recreate-block 100 --hover-write-block 100 --hover-interval 1000 --output-filename $result_path 
        sleep 5
        set +x
    done
done