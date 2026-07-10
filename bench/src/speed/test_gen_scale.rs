use byteorder::{BigEndian, ByteOrder};
use parking_lot::RwLock;
use qmdb::{
    def::{OP_CREATE, OP_READ, OP_WRITE},
    test_helper::{RandSrc, SimpleTask},
    utils::{byte0_to_shard_id, changeset::ChangeSet, hasher},
};

/// TestGenScale - a flexible task generator designed for scale benchmarking.
///
/// Features:
/// 1. Dynamically adjustable batch size - callers can specify any batch size
///    for each `gen_*_tasks` invocation.
/// 2. Incremental data generation - new entries are appended on top of an
///    existing dataset via `add_entries`, with monotonically increasing
///    key numbers starting from `cur_total_count`.
/// 3. Random read/update workload - random read keys and random update
///    key/value pairs are generated using a Zipf distribution to model
///    skewed access patterns.
pub struct TestGenScale {
    pub key_size: usize,
    pub val_size: usize,
    /// Total number of entries that should exist after the latest
    /// `add_entries` call (cumulative across all incremental rounds).
    pub cur_total_count: u64,
    pub block_count: u64,
    zipf_s: f64,
}

impl TestGenScale {
    pub fn new(key_size: usize, val_size: usize, zipf_s: f64) -> Self {
        Self {
            key_size,
            val_size,
            cur_total_count: 0,
            block_count: 0,
            zipf_s,
        }
    }

    /// Incrementally extend the dataset by `count` new entries.
    /// After this call, `cur_total_count` will reflect the new total size.
    pub fn add_entries(&mut self, count: u64) {
        self.cur_total_count += count;
        self.block_count += 1;
    }

    /// Fill key `k` and value `v` for the given `num` (a sequential entry
    /// number). Returns the key hash.
    /// Mirrors `test_gen_micro::fill_kv` so generated keys are compatible
    /// with the rest of the codebase.
    pub fn fill_kv(&self, num: u64, k: &mut [u8], v: &mut [u8]) -> [u8; 32] {
        // key: 000...num (last 4 bytes hold num in big-endian)
        BigEndian::write_u32(&mut k[self.key_size - 4..self.key_size], num as u32);
        let kh = hasher::hash(&k[..]);

        // value: repeat a single character selected by `num`
        static CHARSET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        v[..].fill(CHARSET.chars().nth(num as usize % CHARSET.len()).unwrap() as u8);
        // include the block number in the first 4 bytes for uniqueness
        BigEndian::write_u32(&mut v[0..4], self.block_count as u32);
        kh
    }

    /// Generate `count` new entries (OP_CREATE) and return the corresponding
    /// task list along with the keys/values used. Keys are sequential and
    /// start from the previous `cur_total_count` value.
    pub fn gen_create_tasks(
        &mut self,
        count: usize,
    ) -> (Vec<RwLock<Option<SimpleTask>>>, Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let mut task = RwLock::new(None);
        let mut cset = ChangeSet::new();
        let mut key_list = Vec::with_capacity(count);
        let mut value_list = Vec::with_capacity(count);

        for _ in 0..count {
            let mut k = vec![0u8; self.key_size];
            let mut v = vec![0u8; self.val_size];
            let num = self.cur_total_count;
            let kh = self.fill_kv(num, &mut k[..], &mut v[..]);
            let shard_id = byte0_to_shard_id(kh[0]) as u8;
            cset.add_op(OP_CREATE, shard_id, &kh, &k[..], &v[..], None);
            key_list.push(k);
            value_list.push(v);
            self.cur_total_count += 1;
        }
        cset.sort();
        *task.write() = Some(SimpleTask::new(vec![cset]));
        self.block_count += 1;
        (vec![task], key_list, value_list)
    }

    /// Generate `count` random update (OP_WRITE) operations using a Zipf
    /// distribution to pick the key number. The generated key always refers
    /// to an existing entry (number < `cur_total_count`).
    pub fn gen_random_update_tasks(
        &mut self,
        randsrc: &mut RandSrc,
        count: usize,
    ) -> (Vec<RwLock<Option<SimpleTask>>>, Vec<Vec<u8>>) {
        let mut task = RwLock::new(None);
        let mut cset = ChangeSet::new();
        let mut key_list = Vec::with_capacity(count);

        let zipf = ZipfGenerator::new(self.zipf_s, self.cur_total_count);
        for _ in 0..count {
            let rng_val = randsrc.get_uint64() as f64 / u64::MAX as f64;
            let key_num = zipf.generate(rng_val);

            let mut k = vec![0u8; self.key_size];
            let mut v = vec![0u8; self.val_size];
            // Reuse fill_kv with the current block_count for value uniqueness
            let kh = self.fill_kv(key_num, &mut k[..], &mut v[..]);
            let shard_id = byte0_to_shard_id(kh[0]) as u8;
            cset.add_op(OP_WRITE, shard_id, &kh, &k[..], &v[..], None);
            key_list.push(k);
        }
        cset.sort();
        *task.write() = Some(SimpleTask::new(vec![cset]));
        self.block_count += 1;
        (vec![task], key_list)
    }

    /// Generate `count` random read operations using a Zipf distribution.
    /// Returns the task list along with the keys used for the reads.
    pub fn gen_random_read_tasks(
        &mut self,
        randsrc: &mut RandSrc,
        count: usize,
    ) -> (Vec<RwLock<Option<SimpleTask>>>, Vec<Vec<u8>>) {
        let mut task = RwLock::new(None);
        let mut cset = ChangeSet::new();
        let mut key_list = Vec::with_capacity(count);

        let zipf = ZipfGenerator::new(self.zipf_s, self.cur_total_count);
        for _ in 0..count {
            let rng_val = randsrc.get_uint64() as f64 / u64::MAX as f64;
            let key_num = zipf.generate(rng_val);

            let mut k = vec![0u8; self.key_size];
            // value is required by the signature but unused for read
            let mut v = vec![0u8; self.val_size];
            let kh = self.fill_kv(key_num, &mut k[..], &mut v[..]);
            let shard_id = byte0_to_shard_id(kh[0]) as u8;
            cset.add_op(OP_READ, shard_id, &kh, &k[..], &v[..], None);
            key_list.push(k);
        }
        cset.sort();
        *task.write() = Some(SimpleTask::new(vec![cset]));
        self.block_count += 1;
        (vec![task], key_list)
    }
}

/// Zipf distribution sampler used for generating skewed access patterns.
pub struct ZipfGenerator {
    s: f64,
    n: u64,
    sum: f64,
}

impl ZipfGenerator {
    pub fn new(s: f64, n: u64) -> Self {
        let mut sum = 0.0;
        for i in 1..=n {
            sum += 1.0 / (i as f64).powf(s);
        }
        Self { s, n, sum }
    }

    /// Map a uniform random value in `[0, 1)` to a Zipf-distributed
    /// integer in `[0, n)`.
    pub fn generate(&self, rng_val: f64) -> u64 {
        if self.n == 0 {
            return 0;
        }
        let mut cumulative = 0.0;
        for i in 1..=self.n {
            cumulative += 1.0 / (i as f64).powf(self.s) / self.sum;
            if rng_val <= cumulative {
                return i - 1;
            }
        }
        self.n - 1
    }
}