use std::fs::File;
use std::path::Path;
use std::time::Instant;

use bench::{
    cli::ScaleBenchCli,
    speed::{db_backend, test_gen_scale::TestGenScale},
};

use qmdb::test_helper::RandSrc;

use clap::Parser;
use walkdir::WalkDir;

const N_TABLES: usize = 1;

const SCALES: [u64; 7] = [
    1000, 10000, 100000, 1000000, 10000000, 100000000,
    1000000000,
    // 1000, 10000, 100000, 1000000, 10000000, 100000000, 660000000,
];

fn calculate_dir_size(path: &str) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .fold(0, |acc, m| acc + m.len())
}

fn get_memory_usage() -> Option<(usize, usize)> {
    if let Some(usage) = memory_stats::memory_stats() {
        Some((usage.physical_mem, usage.virtual_mem))
    } else {
        None
    }
}

fn main() {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global()
        .unwrap();

    let args: ScaleBenchCli = ScaleBenchCli::parse();

    println!("config: {:?}", args);

    std::fs::create_dir_all(args.db_dir.as_str()).unwrap();
    std::fs::create_dir_all(args.result_dir.as_str()).unwrap();

    for i in 0..N_TABLES {
        db_backend::init(args.db_dir.as_str(), i);
    }
    println!("Using database backend: {}", db_backend::NAME);

    let table_id = 0;
    let mut randsrc: RandSrc = RandSrc::new(
        args.randsrc_filename.as_str(),
        &format!("qmdb-scale-{}", table_id),
    );

    let output_path = Path::new(&args.result_dir).join(&args.result_file);
    let output_file = File::create(&output_path).unwrap();
    let mut wtr = csv::WriterBuilder::new().from_writer(output_file);

    let headers = [
        "Base Data Scale",
        "Write Batch Size",
        "Read Batch Size",
        "Key Length",
        "Value Length",
        "Write Throughput (records/sec)",
        "Read Throughput (records/sec)",
        "Total Disk Usage (MB)",
        "Average Per Record (bytes)",
        "Memory (MB)",
        "Peak Memory (MB)",
        "Memory Per Record (bytes)",
    ];
    wtr.write_record(&headers).unwrap();

    let empty_db_size = calculate_dir_size(args.db_dir.as_str());
    println!("Empty DB size: {} bytes", empty_db_size);

    // Global task generator - reused across all scale rounds.
    let mut test_gen = TestGenScale::new(
        args.key_length as usize,
        args.value_length as usize,
        args.zipf,
    );

    let mut height = 1;

    for &scale in SCALES.iter() {
        println!("\n=== Scale: {} ===", scale);

        // 1. Incremental data generation: append new entries on top of
        //    whatever was already inserted.
        let incremental_count = scale - test_gen.cur_total_count;
        if incremental_count == 0 {
            println!("No new entries to add at this scale, skipping creation.");
        } else {
            println!("Incremental entries to add: {}", incremental_count);

            // Split the increment into manageable create batches.
            let mut remaining = incremental_count;
            let batch_size = std::cmp::min(100_000, incremental_count) as usize;
            let total_batches = (incremental_count + batch_size as u64 - 1) / batch_size as u64;
            let mut produced_batches = 0u64;
            while remaining > 0 {
                let this_batch = std::cmp::min(batch_size as u64, remaining) as usize;
                let (task_list, _, _) = test_gen.gen_create_tasks(this_batch);
                db_backend::create_kv(table_id, height, task_list);
                height += 1;
                remaining -= this_batch as u64;
                produced_batches += 1;
                println!(
                    "{}%: producing batch {} / {} (entries {} - {})",
                    produced_batches * 100 / total_batches,
                    produced_batches,
                    total_batches,
                    scale - remaining - this_batch as u64,
                    scale - remaining,
                );
            }
            db_backend::flush(table_id);
            println!("Populated {} entries (total: {})", incremental_count, scale);
        }

        let total_db_size = calculate_dir_size(args.db_dir.as_str());
        let net_db_size = total_db_size - empty_db_size;
        let disk_usage_mb = net_db_size as f64 / 1024.0 / 1024.0;
        let avg_per_record = net_db_size as f64 / scale as f64;

        println!(
            "Disk Usage: {:.2} MB, Average per record: {:.2} bytes",
            disk_usage_mb, avg_per_record
        );

        // 2. Random write (update) performance test using Zipf-distributed
        //    keys. Batch size is dynamically adjustable.
        let write_batch_size = args.write_batch as usize;
        let write_start = Instant::now();
        let (write_tasks, _) = test_gen.gen_random_update_tasks(&mut randsrc, write_batch_size);
        db_backend::update_kv(table_id, height, write_tasks);
        db_backend::flush(table_id);
        let write_latency = write_start.elapsed().as_secs_f64();
        let write_throughput = write_batch_size as f64 / write_latency;
        println!("Write Throughput: {:.2} records/sec", write_throughput);
        height += 1;

        // 3. Random read performance test using Zipf-distributed keys.
        //    We only need the keys for a real read; the task list returned
        //    by gen_random_read_tasks is discarded.
        let read_batch_size = args.read_batch as usize;
        let (_read_tasks, read_keys) =
            test_gen.gen_random_read_tasks(&mut randsrc, read_batch_size);
        let read_start = Instant::now();
        let _values = db_backend::read_kv(table_id, height - 1, &read_keys);
        let read_latency = read_start.elapsed().as_secs_f64();
        let read_throughput = read_batch_size as f64 / read_latency;
        println!("Read Throughput: {:.2} records/sec", read_throughput);
        // Note: read_kv is synchronous and does not start a new block,
        // so we do not increment height here.

        let (mem_physical, mem_virtual) = get_memory_usage().unwrap_or((0, 0));
        let mem_mb = mem_physical as f64 / 1024.0 / 1024.0;
        let peak_mem_mb = mem_virtual as f64 / 1024.0 / 1024.0;
        let mem_per_record = mem_physical as f64 / scale as f64;

        println!(
            "Memory: {:.2} MB, Peak Memory: {:.2} MB, Memory per record: {:.2} bytes",
            mem_mb, peak_mem_mb, mem_per_record
        );

        let row = [
            scale.to_string(),
            args.write_batch.to_string(),
            args.read_batch.to_string(),
            args.key_length.to_string(),
            args.value_length.to_string(),
            format!("{:.2}", write_throughput),
            format!("{:.2}", read_throughput),
            format!("{:.2}", disk_usage_mb),
            format!("{:.2}", avg_per_record),
            format!("{:.2}", mem_mb),
            format!("{:.2}", peak_mem_mb),
            format!("{:.2}", mem_per_record),
        ];
        wtr.write_record(&row).unwrap();
        wtr.flush().unwrap();

        println!("Results written to {:?}", output_path);
    }

    println!("\n=== Scale Benchmark Completed ===");
}
