#!/bin/bash
export PATH=$PATH:/home/${USER}/.cargo/bin
cd ../
# cargo clean
cargo build --release
cd exps/

# 定义测试参数数组
load_account=(1000000 10000000)
value_sizes=(1024)
key_size=32
ranges="5,50,100,200,300,400,500,1000,2000"
num_range_test=20
load_batch_size=10000

data_path="$PWD/../data/"
result_dir="${PWD}/results_qmdb/range_benchmark"
mkdir -p $data_path
mkdir -p ${result_dir}
# rm -rf ${result_dir}/*

# 运行测试
for n_acc in "${load_account[@]}"; do
    for value_size in "${value_sizes[@]}"; do
        set -x
        # 清理数据文件夹
        rm -rf $data_path/*
        
        result_path="${result_dir}/e${n_acc}v${value_size}.csv"
        echo $(date "+%Y-%m-%d %H:%M:%S") 
        echo "num account: ${n_acc}, update count:${update_count}, value_size: ${value_size}, key_size: ${key_size}" 
        # 运行测试并提取结果
        ../target/release/range_benchmark --db-dir ${data_path} --entry-count ${n_acc} --ops-per-block ${load_batch_size} --range-list ${ranges} --range-test-count ${num_range_test} --key-size ${key_size} --val-size ${value_size} --output-filename $result_path
        sleep 5
        set +x
    done
done