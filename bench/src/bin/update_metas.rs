use std::panic;
// use std::thread;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::path::Path;
use std::io;
use std::fs::File;

use bench::speed::results::Throughput;
use bench::{
    cli::UpdateMetasCli,
    speed::results::BenchmarkResults,
    speed::{db_backend, test_gen::TestGenV2},
};

use qmdb::def::IN_BLOCK_IDX_BITS;
use qmdb::def::OP_CREATE;
use qmdb::def::OP_DELETE;
use qmdb::def::OP_READ;
use qmdb::def::OP_WRITE;
use qmdb::indexer::hybrid::index_cache::COUNTERS;
use qmdb::test_helper::RandSrc;
use qmdb::test_helper::SimpleTask;
use qmdb::utils::{byte0_to_shard_id, changeset::ChangeSet, hasher};

use byteorder::BigEndian;
use byteorder::ByteOrder;
use clap::Parser;
use log::{info, warn};
use parking_lot::RwLock;
use walkdir::WalkDir;
use csv::{WriterBuilder,Error};

const PRIME1: u64 = 1299827; // Used for stride for hover_recreate_block

const N_TABLES: usize = 1;

fn main() {
    let args: UpdateMetasCli = UpdateMetasCli::parse();
    // let mut results = BenchmarkResults::new(&args);

    // Print config
    println!("config: {:?}", args);

    println!("#Entries to populate DB with: {}", args.entry_count);

    //  Make db_dir if it does not exist
    std::fs::create_dir_all(args.db_dir.as_str()).unwrap();
    for i in 0..N_TABLES {
        db_backend::init(args.db_dir.as_str(), i);
    }
    println!("Using database backend: {}", db_backend::NAME);
    // results.db_backend = db_backend::NAME.to_string();
    // results.log_time("initialized");

    // Calculate tasks (transactions) per block based on desired ops per block
    // Each task contains changesets_per_task changesets
    // Each changeset has ~10 operations (9 writes + 1 delete typically)
    let ops_per_changeset = 10; //test_gen.num_ops_in_cset() as usize;
    let tasks_per_block = args.ops_per_block / (args.changesets_per_task * ops_per_changeset);

    // task_ids encoded the local task_id index in the first IN_BLOCK_IDX_BITS bits of a i64 and
    // the height in the remaining bits. This just makes sure we don't overflow during testing.
    if tasks_per_block >= (1 << IN_BLOCK_IDX_BITS) {
        panic!("tasks_per_block {} is too large", tasks_per_block);
    }
    // Check that it divides evenly
    if args.entry_count % args.ops_per_block != 0 {
        panic!(
            "entry_count {} is not divisible by ops_per_block {}",
            args.entry_count, args.ops_per_block
        );
    }
    let blocks_for_db_population = args.entry_count / args.ops_per_block;
    println!("blocks_for_db_population: {}", blocks_for_db_population);

    println!("Workload configuration:");
    println!(
        "  Changesets per task/transaction: {}",
        args.changesets_per_task
    );
    println!("  Operations per changeset: {}", ops_per_changeset);
    println!("  Tasks (transactions) per block: {}", tasks_per_block);
    println!(
        "  Total operations per block: ~{}",
        tasks_per_block * args.changesets_per_task * ops_per_changeset
    );

    // Future: allow for multiple threads, one per table
    let table_id = 0;
    // TODO: Create a random source file from /dev/urandom if it doesn't exist
    let mut randsrc: RandSrc = RandSrc::new(
        args.randsrc_filename.as_str(),
        &format!("qmdb-{}", table_id),
    );
    // Create test generator with calculated parameters
    let mut test_gen_ = TestGenV2::new(
        &mut randsrc,
        args.entry_count,
        args.changesets_per_task as usize,
        tasks_per_block as usize,
    );
    test_gen_.wr_op_in_cset = ops_per_changeset as usize;
    test_gen_.rd_op_in_cset = 0;
    test_gen_.delete_op_in_cset = 0;
    test_gen_.create_op_in_cset = 0;
    let _ = run(
        0,
        &mut test_gen_,
        // &mut randsrc,
        // &mut results,
        args.tps_blocks,
        // args.hover.hover_interval,
        // args.hover.hover_recreate_block,
        // args.hover.hover_write_block,
        // args.hover.num_read_latency_samples,
        args.db_dir.as_str(),
        args.output_filename.as_str(),
    );
}

fn run(
    table_id: usize,
    test_gen: &mut TestGenV2,
    // randsrc: &mut RandSrc,
    // results: &mut BenchmarkResults,
    tps_blocks: u64,
    // hover_interval: u64,
    // hover_recreate_block: u64,
    // hover_write_block: u64,
    // num_read_latency_samples: u64,
    db_dir: &str,
    output_filename: &str,
)->Result<(), csv::Error> {
    let output_file = File::create(output_filename)?;
    let mut wtr = csv::WriterBuilder::new().from_writer(output_file);
    let mut header: Vec<String> = Vec::new();
    header.push("height".to_string());
    header.push("latency".to_string());
    header.push("throughput".to_string());
    for entry in WalkDir::new(db_dir).max_depth(2).into_iter().filter_map(|e| e.ok()){
        header.push(entry.path().display().to_string());
    }
    wtr.write_record(&header)?;

    // We create 500 blocks every round
    // 50000 tasks in a block * 2cset * (9 writes + 1 delete) = 1 million ops
    // -> 2 million entries per block
    // -> 100 million entries per round
    let blk_in_round = test_gen.block_in_round();
    let start = Instant::now();
    let mut height = 1;

    // Populate the database
    for produced_blocks in 0..blk_in_round {

        let time_left = if produced_blocks > 0 {
            start
                .elapsed()
                .mul_f64(((blk_in_round - produced_blocks) as f64) / (produced_blocks as f64))
        } else {
            Duration::from_secs(0)
        };
        println!(
            "{}%: producing block {} (entry {} - {}) [Time left: {:.2?}]",
            produced_blocks * 100 / blk_in_round,
            produced_blocks,
            produced_blocks * test_gen.num_op_in_blk(),
            (produced_blocks + 1) * test_gen.num_op_in_blk(),
            time_left
        );

        // Generate transactions to populate the database.
        let task_list = test_gen.gen_block();
        let task_count = task_list.len();
        let populate_start = Instant::now();
        db_backend::create_kv(table_id, height, task_list);
        let latency = populate_start.elapsed().as_nanos();
        let throughput = (task_count as f64/latency as f64)*1e9;        

        // logging
        let mut result: Vec<String> = Vec::new();
        result.push(height.to_string());
        result.push(latency.to_string());
        result.push(throughput.to_string());
        println!("produced blocks {}, latency: {}ns, throughput: {:.2?}", produced_blocks, latency, throughput);
        
        // get file sizes
        for entry in WalkDir::new(db_dir).max_depth(2).into_iter().filter_map(|e| e.ok()){
            let total_size = WalkDir::new(entry.path())
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.metadata().ok())
                .filter(|metadata| metadata.is_file())
                .fold(0, |acc, m| acc + m.len());

            println!("{} size: {} bytes.", entry.path().display(), total_size);
            result.push(total_size.to_string());
        }
        wtr.write_record(&result)?;

        height += 1;
        if height % 10 == 0 {
            // Print hybrid cache counters
            COUNTERS.print();
        }
    }

    let _ = wtr.flush();
    println!(
        "Block population complete. Writing partial results to file: {}",
        output_filename
    );

    println!(
        "Benchmarking TPS: {} transactions, {} ops, {} blocks",
        tps_blocks * test_gen.num_txn_in_blk(),
        tps_blocks * test_gen.num_op_in_blk(),
        tps_blocks
    );

    // Benchmarking TPS
    // let mut transactions_performed: u64 = 0;
    for b in 0..tps_blocks {
        // Each transaction is a task
        // task_count is the number of transactions
        let task_list = test_gen.gen_block();
        let task_count = task_list.len();
        let tps_start = Instant::now();
        db_backend::update_kv(table_id, height, task_list);
        let latency = tps_start.elapsed().as_nanos();
        let throughput = (task_count as f64/latency as f64)*1e9;

        // logging
        let mut result: Vec<String> = Vec::new();
        result.push(height.to_string());
        result.push(latency.to_string());
        result.push(throughput.to_string());
        println!("TPS block {}, task count: {}, latency: {}ns, throughput: {:.2?}", b, task_count, latency, throughput);
        // 获取目录大小
        for entry in WalkDir::new(db_dir).max_depth(2).into_iter().filter_map(|e| e.ok()){
            let total_size = WalkDir::new(entry.path())
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.metadata().ok())
                .filter(|metadata| metadata.is_file())
                .fold(0, |acc, m| acc + m.len());

            println!("{} size: {} bytes.", entry.path().display(), total_size);
            result.push(total_size.to_string());
        }
        wtr.write_record(&result)?;
        height += 1;
    }
    println!("Benchmarking completed successfully");
    let _ = wtr.flush();
    println!("Writing results to file: {}", output_filename);
    Ok(())
}
