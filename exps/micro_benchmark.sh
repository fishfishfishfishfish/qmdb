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
# entries_counts=(1000000 2000000 3000000 4000000 5000000 6000000 7000000 8000000 9000000 10000000 20000000 30000000 40000000 50000000 60000000 70000000 80000000 90000000 100000000 500000000 1000000000)
entries_counts=(10000 100000 1000000 10000000)
# entries_counts=(1002000)
# batch_sizes=(3000)
# batch_sizes=(500 1000 2000 4000 5000)
# value_sizes=(256 512 1024 2048)
batch_sizes=(2000)
value_sizes=(256)
tps_blocks=0
key_size=32

data_path="${PWD}/data"
result_dir="${PWD}/results_qmdb/micro_benchmark"
mkdir -p ${result_dir}
rm -rf ${result_dir}/*

for n_acc in "${entries_counts[@]}"; do
    for ops_per_block in "${batch_sizes[@]}"; do
        for value_size in "${value_sizes[@]}"; do
            set -x
            rm -rf ${data_path}
            ../target/release/micro_benchmark --db-dir ${data_path} --tps-blocks ${tps_blocks} --entry-count ${n_acc} --ops-per-block ${ops_per_block} --key-size ${key_size} --val-size ${value_size} --output-filename "${result_dir}/e${n_acc}b${ops_per_block}v${value_size}.csv"
            
            # 检查文件夹是否存在
            if [ ! -d "$data_path" ]; then
                echo "数据文件夹 $data_path 不存在。"
                exit 1
            fi
            # 获取文件夹大小
            folder_size=$(du -sk "$data_path" | cut -f1)
            # 输出结果
            echo "${n_acc}, ${ops_per_block}, ${value_size}, ${key_size}, ${folder_size}" >> "${result_dir}/size"  
            sleep 5
            set +x
        done
    done
done