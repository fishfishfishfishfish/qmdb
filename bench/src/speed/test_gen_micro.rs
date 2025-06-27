use byteorder::{BigEndian, ByteOrder};
use parking_lot::RwLock;
use qmdb::{
    def::{OP_CREATE, OP_READ, OP_WRITE},
    test_helper::{RandSrc, SimpleTask},
    utils::{byte0_to_shard_id, changeset::ChangeSet, hasher},
};

use super::shuffle_param::ShuffleParam;

const PRIME2: u64 = 888869;

#[derive(Debug, Clone)]
pub struct TestGenMicro {
    pub key_size: usize,
    pub val_size: usize,
    pub entry_count: u64,
    pub ops_in_cset: usize,
    pub cset_in_task: usize,
    pub task_in_block: usize,
    cur_read_num: u64,
    cur_update_num: u64,
    pub cur_num: u64,
    pub cur_round: usize,
    block_count: u64,
    pub sp: ShuffleParam,
}

// TODO: Refactor this to avoid using different rounds for populating the database and benchmarking TPS
impl TestGenMicro {
    pub fn new(
        randsrc: &mut RandSrc,
        entry_count: u64,
        ops_in_cset: usize,
        cset_in_task: usize,
        task_in_block: usize,
    ) -> Self {
        // all entries are all will-not-delete
        let entry_count_wo_del = entry_count;
        let mut sp = ShuffleParam::new(entry_count, entry_count_wo_del);
        sp.add_num = randsrc.get_uint64();
        sp.xor_num = randsrc.get_uint64();
        sp.rotate_bits = randsrc.get_uint32() as usize % sp.total_bits;

        Self {
            key_size: 32,
            val_size: 1024,
            entry_count,
            // number of operations in a changeset
            ops_in_cset,
            // Number of changesets in a task
            cset_in_task,
            // Number of tasks in a block
            task_in_block,
            // Counters
            cur_read_num: 0,
            cur_update_num: 0,
            cur_num: 0,
            cur_round: 0,
            block_count: 0,
            sp,
        }
    }

    pub fn num_cset_in_blk(&self) -> u64 {
        // number of cset in a block
        self.task_in_block as u64 * self.cset_in_task as u64
    }

    pub fn num_ops_in_cset(&self) -> u64 {
        // This is for the first round, where we are populating the database
        self.ops_in_cset as u64
    }

    pub fn num_op_in_blk(&self) -> u64 {
        self.task_in_block as u64 * self.cset_in_task as u64 * self.num_ops_in_cset()
    }

    pub fn block_in_round(&self) -> u64 {
        // load all the entries in a round
        let op_in_blk = self.num_op_in_blk();
        if self.entry_count % op_in_blk != 0 {
            panic!(
                "invalid entry_count {} % {} != 0",
                self.entry_count, op_in_blk
            );
        }
        let result = self.entry_count / op_in_blk;
        if result == 0 {
            panic!("entry_count not enough");
        }
        result
    }

    pub fn gen_block(&mut self) -> (Vec<RwLock<Option<SimpleTask>>>, Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let blk_in_round = self.block_in_round();
        // if self.block_count == blk_in_round{
        if self.block_count != 0 && self.block_count % blk_in_round == 0 {
            // First round is to populate the database, second round is to benchmark TPS.
            // println!(
            //     "New round: {} -> {}, cur_num: {},  cur_update_num: {}, cur_read_num: {}",
            //     self.cur_round, self.cur_round+1, self.cur_num, self.cur_update_num, self.cur_read_num
            // );
            self.cur_round += 1;
            self.cur_num = 0;
            self.cur_read_num = 0;
            self.cur_update_num = 0;
        }
        //println!("AA gen_block cur_round={} block_count={} sp={:?}", self.cur_round, self.block_count, self.sp);
        let mut simple_tasks: Vec<RwLock<Option<SimpleTask>>> =
            Vec::with_capacity(self.task_in_block);
        let mut key_list = Vec::new();
        let mut value_list = Vec::new();
        for _ in 0..self.task_in_block {
            simple_tasks.push(RwLock::new(None));
        }
        for j in 0..self.task_in_block {
            let idx = j;
            let mut task = simple_tasks[idx].write();
            let (simple_t, kl, vl) = self.gen_task();
            *task = Some(simple_t);
            key_list.extend(kl);
            value_list.extend(vl);
        }
        self.block_count += 1;
        (simple_tasks, key_list, value_list)
    }

    fn gen_task(&mut self) -> (SimpleTask, Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let mut v = Vec::with_capacity(self.cset_in_task);
        let mut key_list = Vec::new();
        let mut value_list = Vec::new();
        for _ in 0..self.cset_in_task {
            let (cset, kl, vl) = self.gen_cset();
            v.push(cset);
            key_list.extend(kl);
            value_list.extend(vl);
        }
        (SimpleTask::new(v), key_list, value_list)
    }

    pub fn fill_kv(&self, _op_type: u8, num: u64, k: &mut [u8], v: &mut [u8]) -> [u8; 32] {
        // the key is 000...num, where num is a 32-bit number.
        BigEndian::write_u32(&mut k[self.key_size - 4..self.key_size], num as u32);
        // 将num转换为字符串，并指定长度和补零
        // let num_str = format!("{:0>width$}", num, width = self.key_size);
        // // 将字符串写入k中
        // k[..self.key_size].copy_from_slice(num_str.as_bytes());
        // let hash = hasher::hash(&k[self.key_size-4..self.key_size]);
        // if self.key_size-4 < 32 {
        //     k[0..self.key_size-4].copy_from_slice(&hash[0..self.key_size-4]);
        // } else{
        //     k[0..32].copy_from_slice(&hash[0..32]);
        //     for i in 32..self.key_size-4 {
        //         // fill zero bytes with pseudo-random values
        //         k[i] = k[20 + i] ^ k[32 + i];
        //     }
        // }
        let kh = hasher::hash(&k[..]);

        // the value repeats a char
        static CHARSET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        v[..].fill(CHARSET.chars().nth(num as usize % CHARSET.len()).unwrap() as u8);
        BigEndian::write_u32(&mut v[0..4], self.block_count as u32);
        // BigEndian::write_u32(&mut v[..4], self.cur_round as u32);
        // v[0..].copy_from_slice(&kh[4..]);
        kh
    }

    fn gen_cset(&mut self) -> (ChangeSet, Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let num_ops = self.num_ops_in_cset();
        let mut cset = ChangeSet::new();
        let mut key_list = Vec::with_capacity(num_ops as usize);
        let mut value_list = Vec::with_capacity(num_ops as usize);

        if self.cur_round == 0 {
            // loading data
            for _ in 0..num_ops {
                let mut k = vec![0u8; self.key_size];
                let mut v = vec![0u8; self.val_size];
                let num = self.cur_num;
                let kh = self.fill_kv(OP_CREATE, num, &mut k[..], &mut v[..]);
                let shard_id = byte0_to_shard_id(kh[0]) as u8;
                //let k64 = BigEndian::read_u64(&kh[0..8]);
                // println!("AA blkcnt={:#04x} r={:#04x} op={} cur_num={:#08x} num={:#08x} k64={:#016x} k={:?} kh={:?}", self.block_count, self.cur_round, op_type, self.cur_num, num, k64, k, kh);
                // println!("AA blkcnt={} r={} op={} cur_num={} num={}, shard_id={}", self.block_count, self.cur_round, op_type, self.cur_num, num, shard_id);
                // println!("AA blkcnt={} k={:?}", self.block_count, &k[..]);
                self.cur_num += 1;

                cset.add_op(OP_CREATE, shard_id, &kh, &k[..], &v[..], None);
                key_list.push(k);
                value_list.push(v);
            }
        } else {
            // update entries
            for _ in 0..num_ops {
                let mut k = vec![0u8; self.key_size];
                let mut v = vec![0u8; self.val_size];
                let num = self.cur_update_num % self.entry_count;
                let num = self.sp.change(num);
                let kh = self.fill_kv(OP_WRITE, num, &mut k[..], &mut v[..]);
                let shard_id = byte0_to_shard_id(kh[0]) as u8;
                self.cur_update_num += 1;

                cset.add_op(OP_WRITE, shard_id, &kh, &k[..], &v[..], None);
                key_list.push(k);
                value_list.push(v);
            }
        }
        cset.sort();
        (cset, key_list, value_list)
    }
}
