# Scale Benchmark 测试文档

## 测试目的

Scale Benchmark 是一个**数据量增长测试**，用于评估数据库在不同数据规模下的性能表现，包括写入/读取吞吐量、磁盘空间占用和内存使用情况。

---

## 测试配置参数

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--db-dir` | `testdata/paper` | 数据库存储路径 |
| `--write-batch` | 1000 | 写入吞吐量测试的记录数 |
| `--read-batch` | 1000 | 读取吞吐量测试的记录数 |
| `--key-length` | 32 bytes | 键的长度 (字节) |
| `--value-length` | 1024 bytes | 值的长度 (字节) |
| `--result-dir` | `exps/results_ledgerdb` | 结果目录路径 |
| `--result-file` | `scaleBenchmark.csv` | 结果文件名 |
| `--zipf` | 0.99 | Zipf 分布参数（越接近1越倾斜） |

---

## 测试数据规模

测试按 **7 个递进尺度**进行，数据量从 1K 增长到 1B：

```
1000 → 10000 → 100000 → 1000000 → 10000000 → 100000000 → 1000000000
   1K       10K        100K         1M          10M           100M          1B
```

采用**增量写入**方式：每个尺度仅写入与上一尺度的差值（如从 1M 到 10M 只写入 9M 条记录）。

---

## 测试流程

每个数据尺度的测试步骤：

1. **写入增量数据**：使用 `MicroWrite4Growth` 写入该尺度的增量数据，记录加载吞吐量
2. **计算磁盘使用**：调用 `calculateDirSize` 计算总磁盘占用，换算为 MB 和每记录平均字节数
3. **测试写入吞吐量**：使用 **Zipf 分布**生成热点 key，调用 `UpdateBatchKey` 更新测试，计算 records/sec
4. **测试读取吞吐量**：使用相同的 Zipf key，调用 `ReadBatchKey` 读取测试，计算 records/sec
5. **获取内存使用**：读取 `/proc/self/statm` 获取进程内存（当前/峰值/每记录）
6. **输出结果**：打印到控制台并写入 CSV 文件

---

## 测量指标

CSV 输出包含 12 列：

| 列名 | 说明 |
|------|------|
| `Base Data Scale` | 当前数据规模 |
| `Write Batch Size` | 写入批次大小 |
| `Read Batch Size` | 读取批次大小 |
| `Key Length` | 键长度 (字节) |
| `Value Length` | 值长度 (字节) |
| `Write Throughput (records/sec)` | 写入吞吐量 |
| `Read Throughput (records/sec)` | 读取吞吐量 |
| `Total Disk Usage (MB)` | 总磁盘占用 |
| `Average Per Record (bytes)` | 每记录平均磁盘占用 |
| `Memory (MB)` | 当前内存使用 |
| `Peak Memory (MB)` | 峰值内存使用 |
| `Memory Per Record (bytes)` | 每记录平均内存占用 |

---

## 关键实现细节

### Zipf 分布访问

读写测试使用 Zipf 分布模拟热点数据访问，参数 `--zipf=0.99` 表示高度倾斜的访问模式：

```go
zipfKeys := genRandomKeyByZipf(zipfType, batchSize, int(currentSize), zipf)
```

### 分区计数监控

测试前后打印分区数量变化，用于观察数据库在数据增长过程中的分区行为：

```go
fmt.Printf("write before partition count: %d\n", getPartitionCount(dataPath))
// ... write test ...
fmt.Printf("write after partition count: %d\n", getPartitionCount(dataPath))
```

### 内存统计方式

通过读取 `/proc/self/statm` 获取进程内存信息：

- **驻留内存 (RSS)**：当前实际使用的物理内存
- **虚拟内存 (VSZ)**：进程的虚拟内存大小（作为峰值内存近似）
- **页面大小**：通过 `os.Getpagesize()` 获取（通常为 4KB）

### 空库基准

记录初始空数据库大小，用于计算净数据占用：

```go
emptyDbSize := calculateDirSize(dataPath)
// ...
avgPerRecord = float64(totalDbSize-emptyDbSize) / float64(currentSize)
```

---

## 使用方式

```bash
letus-vidb scaleBenchmark --db-dir=/path/to/db --result-dir=results
```

### 完整示例

```bash
letus-vidb scaleBenchmark \
    --db-dir=/data/vidb/test \
    --cacheCost=$((10*1024*1024*1024)) \
    --VlogSize=10 \
    --write-batch=10000 \
    --read-batch=10000 \
    --key-length=32 \
    --value-length=1024 \
    --result-dir=exps/results \
    --result-file=scale_test.csv \
    --zipf=0.99
```

---

## 结果文件

测试结果自动保存到 `{result-dir}/{result-file}`：

```
exps/results_ledgerdb/scaleBenchmark.csv
```

文件格式为 CSV，可直接用 Excel、Python pandas 等工具进行分析和可视化。

---

## 相关代码

- **主测试逻辑**：`cmd/scaleBenchmark.go`
- **内存统计**：`getMemoryUsage()` 函数
- **分区计数**：`getPartitionCount()` 函数
- **磁盘计算**：`calculateDirSize()` 函数（外部引用）
