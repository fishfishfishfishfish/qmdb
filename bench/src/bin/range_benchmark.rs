use std::iter;
use std::panic;
// use std::thread;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use bench::speed::results::Throughput;
use bench::{
    cli::RangeBenchCli,
    speed::{db_backend, test_gen_micro::TestGenMicro},
};

use qmdb::def::IN_BLOCK_IDX_BITS;
use qmdb::def::OP_CREATE;
use qmdb::def::OP_DELETE;
use qmdb::def::OP_READ;
use qmdb::def::OP_WRITE;
use qmdb::indexer::hybrid::index_cache::COUNTERS;
use qmdb::tasks::Task;
use qmdb::test_helper::RandSrc;
use qmdb::test_helper::SimpleTask;
use qmdb::utils::{byte0_to_shard_id, changeset::ChangeSet, hasher};

use byteorder::BigEndian;
use byteorder::ByteOrder;
use clap::Parser;
use csv::{Error, WriterBuilder};
use log::{info, warn};
use parking_lot::RwLock;
use serde::de::value;
use walkdir::WalkDir;

const PRIME1: u64 = 1299827; // Used for stride for hover_recreate_block

const N_TABLES: usize = 1;

fn main() {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global()
        .unwrap();

    let args: RangeBenchCli = RangeBenchCli::parse();
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
    let mut test_gen_ = TestGenMicro::new(
        &mut randsrc,
        args.entry_count,
        ops_per_changeset as usize,
        args.changesets_per_task as usize,
        tasks_per_block as usize,
    );
    test_gen_.key_size = args.key_size as usize;
    test_gen_.val_size = args.val_size as usize;

    let _ = run(
        0,
        &mut test_gen_,
        args.range_list,
        args.range_test_count,
        args.output_filename.as_str(),
    );
}

fn run(
    table_id: usize,
    test_gen: &mut TestGenMicro,
    range_list: Vec<u64>,
    range_test_count: u64,
    output_filename: &str,
) -> Result<(), csv::Error> {
    let output_file = File::create(output_filename)?;
    let mut wtr = csv::WriterBuilder::new().from_writer(output_file);
    let mut header: Vec<String> = Vec::new();
    header.push("height".to_string());
    header.push("range".to_string());
    header.push("latency".to_string());
    header.push("throughput".to_string());
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
        let (task_list, _, _) = test_gen.gen_block();
        let task_count = task_list.len();
        let populate_start = Instant::now();
        db_backend::create_kv(table_id, height, task_list);
        let latency = populate_start.elapsed().as_nanos();
        let throughput = (task_count as f64 / latency as f64) * 1e9;

        // logging
        let mut result: Vec<String> = Vec::new();
        result.push(height.to_string());
        result.push("-1".to_string());
        result.push(latency.to_string());
        result.push(throughput.to_string());
        println!(
            "produced blocks {} , latency: {}ns, throughput: {:.2?}",
            produced_blocks, latency, throughput
        );
        wtr.write_record(&result)?;

        height += 1;
        if height % 10 == 0 {
            // Print hybrid cache counters
            COUNTERS.print();
        }
    }

    db_backend::flush(table_id);
    wtr.flush()?;
    println!(
        "Block population complete. Writing partial results to file: {}",
        output_filename
    );

    height -= 1;
    for range_size in range_list {
        for b in 0..range_test_count {
            let num = test_gen.sp.change(b);
            let mut key_list = Vec::with_capacity(range_size as usize);
            let mut val_list = Vec::with_capacity(range_size as usize);
            for r in 0..range_size {
                let mut k = vec![0u8; test_gen.key_size];
                let mut v = vec![0u8; test_gen.val_size];
                test_gen.fill_kv(OP_READ, num + r, &mut k[..], &mut v[..]);
                key_list.push(k);
                val_list.push(v);
            }
            let key_count = key_list.len();

            let get_start = Instant::now();
            let value_list_2 = db_backend::read_kv(table_id, -1, &key_list);
            let get_latency = get_start.elapsed().as_nanos() as f64 * 1e-9;
            let get_throughput = key_count as f64 / get_latency as f64;
            for v in 0..key_count {
                // println!("v1: {:?}", value_list[v]);
                // println!("v2: {:?}", value_list_2[v]);
                assert_eq!(
                    val_list[v][5..test_gen.val_size - 5],
                    value_list_2[v][5..test_gen.val_size - 5]
                )
            }

            // logging
            let mut result: Vec<String> = Vec::new();
            result.push(height.to_string());
            result.push(range_size.to_string());
            result.push(get_latency.to_string());
            result.push(get_throughput.to_string());
            wtr.write_record(result)?;
            println!(
                "height {}, range size: {}, latency: {} s, throughput: {:.2?}",
                height, range_size, get_latency, get_throughput,
            );
        }
    }
    println!("Benchmarking range completed successfully");
    wtr.flush()?;

    println!("Writing results to file: {}", output_filename);
    drop(wtr);
    Ok(())
}
