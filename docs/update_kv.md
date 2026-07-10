以下是 `db_backend::update_kv` 函数的完整调用链分析：

## `db_backend::update_kv` 函数作用与调用链

### 1. 该函数的作用

[`db_backend::update_kv`](../bench/src/speed/db_backend.rs#L47-L60) 是一个**抽象层分发函数**，它根据启用的 backend 特性（feature flags）将"更新 KV"的请求转发到对应的具体实现。


当前构建使用的是 **QMDB 后端**（`cfg(all(not(feature = "use_mdbx"), not(feature = "use_rocksdb")))` 分支），所以会调用 `crate::speed::qmdb::update_kv`。

### 2. 调用链（从入口到最终执行）

```
scale_benchmark.rs:146
   ↓ db_backend::update_kv(table_id, height, write_tasks)
db_backend.rs:58-60
   ↓ 转发到 qmdb 后端
qmdb.rs:46-48
   ↓ update_kv() 内部直接调用 create_kv() （共用了同一个底层实现！）
qmdb.rs:27-44  create_kv()
   ↓ 1) ads.start_block(height, TasksManager::new(...))
   ↓ 2) shared_ads.insert_extra_data(height, "")
   ↓ 3) for each task: shared_ads.add_task(task_id)
AdsCore::start_block (../qmdb/src/lib.rs#L756)
   ↓ 通过中间层 AdsWrap → AdsCore
start_block 内部:
   - 校验 height (../qmdb/src/lib.rs#L761-763)：
     若 `height == self.stop_height + 1`，说明到达停止高度，立即返回 `(false, None)`，
     调用方应据此停止提交新 block。
   - 分配新 EntryCache (../qmdb/src/lib.rs#L764 `self.cache = self.allocate_cache();`)：
     为新 block 准备一个 entry 缓存，后续该 block 的 updater 会使用此 cache 读写 entry。
   - 检查 task_hub 是否有空闲 slot (../qmdb/src/lib.rs#L767 `self.task_hub.free_slot_count()`)：
     BlockPairTaskHub 维护两个 slot，若两个都已被占用（说明后台消费跟不上），
     主线程必须阻塞等待后台完成一个 block 后才能继续。
   - 阻塞等待 end_block_chan (../qmdb/src/lib.rs#L769 `self.end_block_chan.recv().unwrap()`)：
     在 end_block_chan 上 `recv()` 阻塞；后台线程处理完一个 block 后会通过此 channel 发送 `MetaInfo`，
     携带 `curr_height` 等信息。主线程在此同步点与后台消费者对齐节奏。
updater.rs:137-143
   ↓ 根据 op_type 分发
   ↓ OP_CREATE  → create_kv (创建新条目)
   ↓ OP_WRITE   → write_kv (更新已有条目)
   ↓ OP_DELETE  → delete_kv
   ↓ OP_READ    → noop (实际读取由 read_entry 完成)
```

### 3. 关键代码解析

**入口（[db_backend.rs:47-60](../bench/src/speed/db_backend.rs#L47-L60)）：**
```rust
#[cfg(feature = "use_mdbx")]
pub fn update_kv(...) { crate::speed::mdbx::update_kv(...); }

#[cfg(feature = "use_rocksdb")]
pub fn update_kv(...) { crate::speed::rocksdb::update_kv(...); }

#[cfg(all(not(feature = "use_mdbx"), not(feature = "use_rocksdb")))]
pub fn update_kv(tid: usize, height: i64, task_list: Vec<...>) {
    crate::speed::qmdb::update_kv(tid, height, task_list);
}
```

**QMDB 实现（[qmdb.rs:46-48](../bench/src/speed/qmdb.rs#L46-L48)）：**
```rust
pub fn update_kv(tid: usize, height: i64, task_list: Vec<RwLock<Option<SimpleTask>>>) {
    create_kv(tid, height, task_list);  // 直接复用 create_kv
}
```
⚠️ **注意**：`update_kv` 实际上就是调用 `create_kv`！它只是复用 `create_kv` 的代码。区别在于调用方传入的 task 中包含不同的 op_type（CREATE vs WRITE/READ/DELETE）。

**核心 `create_kv` 实现（[qmdb.rs:27-44](../bench/src/speed/qmdb.rs#L27-L44)）：**
```rust
pub fn create_kv(tid: usize, height: i64, task_list: Vec<...>) {
    let task_count = task_list.len() as i64;
    let last_task_id = (height << IN_BLOCK_IDX_BITS) | (task_count - 1);
    let mut ads = unsafe { ADS[tid].take().unwrap() };  // 取出全局 ADS

    // 1. 启动一个新的 block
    ads.start_block(height, Arc::new(TasksManager::new(task_list, last_task_id)));

    // 2. 获取共享引用
    let shared_ads = ads.get_shared();
    shared_ads.insert_extra_data(height, "".to_owned());

    // 3. 将每个 task 注册到 task_hub 中
    for idx in 0..task_count {
        let task_id = (height << IN_BLOCK_IDX_BITS) | idx;
        shared_ads.add_task(task_id);
    }

    unsafe { ADS[tid] = Some(ads); }  // 放回全局 ADS
}
```

### 4. 关键观察

1. **ADS 是全局单例**：使用 `pub static mut ADS: [Option<AdsWrap<...>>; 2]` 来保存 ADS 实例，通过 `take()` 和 `Some(ads)` 来"借用-归还"模式访问。

2. **task_id 编码**：`(height << IN_BLOCK_IDX_BITS) | idx` —— 使用 height 和 task 索引组合成唯一的 task_id。

3. **`update_kv` 复用 `create_kv`**：因为底层通过 ChangeSet 中的 op_type 来区分 CREATE/WRITE/READ/DELETE，不需要区分的入口函数。

4. **真正的执行是异步的**：`start_block` 只是注册任务，真正的处理由后台工作线程（task_hub、flusher）执行。

5. **`flush` 必须调用**：调用 `db_backend::update_kv` 后必须调用 `db_backend::flush(table_id)` 才能确保数据落盘（`ads.flush()` 等待所有任务完成）。

### 5. 总结

`db_backend::update_kv` 本质上是一个**任务注册入口**，它：
- 将传入的 task_list 注册到全局 ADS 实例中
- 启动一个新的 block（使用 height 标识）
- 后台线程会在适当时候处理这些 task（执行 ChangeSet 中的每个操作）
- 真正的数据写入/读取操作发生在后台工作线程中