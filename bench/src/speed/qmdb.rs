use std::sync::{Arc, Mutex};

use parking_lot::RwLock;
use qmdb::{
    config::Config,
    def::{DEFAULT_ENTRY_SIZE, IN_BLOCK_IDX_BITS},
    entryfile::EntryBz,
    tasks::TasksManager,
    test_helper::SimpleTask,
    utils::hasher,
    AdsCore, AdsWrap, SharedAdsWrap, ADS,
};

pub static mut ADS: [Option<AdsWrap<SimpleTask>>; 2] = [None, None];

pub fn init(qmdb_dir: &str, table_id: usize) {
    // init QMDB
    let qmdb_dir_with_table_id = format!("{}-{}", qmdb_dir, table_id);
    let config = Config::from_dir(&qmdb_dir_with_table_id);
    AdsCore::init_dir(&config);
    let ads = AdsWrap::<SimpleTask>::new(&config);
    unsafe {
        ADS[table_id] = Some(ads);
    }
}

pub fn create_kv(tid: usize, height: i64, task_list: Vec<RwLock<Option<SimpleTask>>>) {
    let task_count = task_list.len() as i64;
    let last_task_id = (height << IN_BLOCK_IDX_BITS) | (task_count - 1);
    let mut ads = unsafe { ADS[tid].take().unwrap() };
    //warmup_indexer(&indexer, height, &task_list);
    //fake_indexer(&idx_file, page_count, &task_list);
    ads.start_block(height, Arc::new(TasksManager::new(task_list, last_task_id)));
    let shared_ads = ads.get_shared();
    shared_ads.insert_extra_data(height, "".to_owned());
    for idx in 0..task_count {
        let task_id = (height << IN_BLOCK_IDX_BITS) | idx;
        //println!("AA bench height={} task_id={:#08x}", height, task_id);
        shared_ads.add_task(task_id);
    }
    unsafe {
        ADS[tid] = Some(ads);
    }
}

pub fn update_kv(tid: usize, height: i64, task_list: Vec<RwLock<Option<SimpleTask>>>) {
    create_kv(tid, height, task_list);
}

pub fn delete_kv(tid: usize, height: i64, task_list: Vec<RwLock<Option<SimpleTask>>>) {
    create_kv(tid, height, task_list);
}

pub fn flush(tid: usize) {
    let mut ads = unsafe { ADS[tid].take().unwrap() };
    ads.flush();
    unsafe {
        ADS[tid] = Some(ads);
    }
}

pub fn get_ads(tid: usize) -> AdsWrap<SimpleTask> {
    unsafe { ADS[tid].take().unwrap() }
}

pub fn return_ads(tid: usize, ads: AdsWrap<SimpleTask>) {
    unsafe {
        ADS[tid] = Some(ads);
    }
}

//let shared_ads = &ads.get_shared();
// pub fn read_kv(shared_ads: &SharedAdsWrap, key_list: &Vec<[u8; 52]>) {
pub fn read_kv(tid: usize, height: i64, key_list: &Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let ads = unsafe { ADS[tid].take().unwrap() };
    let shared_ads = ads.get_shared();
    let values_list = Arc::new(Mutex::new(Vec::new()));

    // Clone the Arc before moving it into the closure
    let cloned_values_list = Arc::clone(&values_list);
    // rayon::scope(|s| {
    //     s.spawn(move |_| {
    let mut buf = [0; DEFAULT_ENTRY_SIZE];
    for k in key_list.iter() {
        let kh = hasher::hash(&k[..]);
        // println!("AA read k={:?}, kh={:?} ", k, kh);
        let (size, ok) = shared_ads.read_entry(height, &kh[..], &k[..], &mut buf);
        if !ok {
            panic!("Cannot read entry k={:?}, kh={:?} ", k, kh);
        }
        let entry_bz = EntryBz { bz: &buf[..size] };
        let value = entry_bz.value().to_vec();
        // println!("AA read k={:?}, kh={:?} value={:?} ", k, kh, value);
        cloned_values_list.lock().unwrap().push(value);
    }
    //     });
    // });
    unsafe {
        ADS[tid] = Some(ads);
    }
    let result = values_list.lock().unwrap().clone();
    result
}
