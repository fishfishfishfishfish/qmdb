#!/bin/bash

# ================================================
# ViDB Scale Benchmark 启动脚本
# 用于运行 scaleBenchmark 性能测试
# ================================================

export PATH=$PATH:/home/${USER}/.cargo/bin
# 脚本配置
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_BIN="$SCRIPT_DIR/../target/release/scale_benchmark"
DEFAULT_DB_DIR="$SCRIPT_DIR/data/"
DEFAULT_RESULT_DIR="$SCRIPT_DIR/results_qmdb/scale_benchmark/"
DEFAULT_RESULT_FILE="scaleBenchmark_${TIMESTAMP}.csv"
CLEAN_DB="${CLEAN_DB:-true}"
LOG_DIR="$SCRIPT_DIR/logs"
LOG_FILE="$LOG_DIR/scale_benchmark_${TIMESTAMP}.log"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 日志初始化
init_logging() {
    mkdir -p "$LOG_DIR"
    echo "=== ViDB Scale Benchmark 运行日志 ===" > "$LOG_FILE"
    echo "开始时间: $(date)" >> "$LOG_FILE"
    echo "脚本目录: $SCRIPT_DIR" >> "$LOG_FILE"
    echo "========================================" >> "$LOG_FILE"
    echo "" >> "$LOG_FILE"
}

log_message() {
    local level="$1"
    local message="$2"
    local timestamp="$(date '+%Y-%m-%d %H:%M:%S')"
    
    echo "[$timestamp] [$level] $message" >> "$LOG_FILE"
}

# 打印带颜色的消息并记录到日志
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
    log_message "INFO" "$1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
    log_message "INFO" "$1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
    log_message "WARNING" "$1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
    log_message "ERROR" "$1"
}

# 构建项目
build_project() {
    print_info "构建项目..."
    cd "$SCRIPT_DIR/.."
    cargo clean
    cargo build --release 2>&1 | tee -a "$LOG_FILE"
    cd "$SCRIPT_DIR"
    
    if [[ ! -f "$BENCHMARK_BIN" ]]; then
        print_error "构建失败，可执行文件不存在: $BENCHMARK_BIN"
        exit 1
    fi
    print_success "项目构建成功"
}

# 检查可执行文件是否存在
check_binary() {
    if [[ ! -f "$BENCHMARK_BIN" ]]; then
        print_info "可执行文件不存在，开始构建..."
        build_project
    fi
    
    if [[ ! -x "$BENCHMARK_BIN" ]]; then
        print_error "可执行文件没有执行权限: $BENCHMARK_BIN"
        chmod +x "$BENCHMARK_BIN"
        print_success "已添加执行权限"
    fi
}

# 显示使用说明
show_usage() {
    echo "用法: $0 [模式] [参数]"
    echo ""
    echo "模式:"
    echo "  default       使用默认参数运行 (推荐)"
    echo "  custom        使用自定义参数运行"
    echo "  quick         快速测试模式 (小数据量)"
    echo "  stress        压力测试模式 (大数据量)"
    echo "  help          显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 default                    # 默认测试"
    echo "  $0 quick                      # 快速测试"
    echo "  $0 custom --write-batch=5000  # 自定义参数"
    echo ""
    echo "自定义参数示例:"
    echo "  --write-batch=NUM    写入批次大小 (默认: 10000)"
    echo "  --read-batch=NUM     读取批次大小 (默认: 10000)"  
    echo "  --key-length=NUM     键长度 (默认: 32)"
    echo "  --value-length=NUM   值长度 (默认: 512)"
    echo "  --zipf=NUM           Zipf分布参数 (默认: 0.99)"
    echo "  --db-dir=PATH        数据库目录 (默认: ./data)"
    echo "  --result-file=PATH   结果文件 (默认: scaleBenchmark.csv)"
}


# 创建测试目录
create_test_dir() {
    local db_dir="${1:-$DEFAULT_DB_DIR}"
    
    if [[ ! -d "$db_dir" ]]; then
        print_info "创建数据库目录: $db_dir"
        mkdir -p "$db_dir"
    fi
    
    if [[ "$CLEAN_DB" == "true" ]]; then
        print_warning "清理旧数据库数据..."
        rm -rf "$db_dir"/*
    fi
}

# 默认模式运行
default_mode() {
    print_info "运行默认测试模式..."
    create_test_dir "$DEFAULT_DB_DIR"
    
    local args=("--write-batch=10000" 
                "--read-batch=10000" 
                "--key-length=32" 
                "--value-length=512"
                "--zipf=0.99"
                "--db-dir=$DEFAULT_DB_DIR"
                "--randsrc-filename=$SCRIPT_DIR/randsrc.dat"
                "--result-dir=$DEFAULT_RESULT_DIR"
                "--result-file=$DEFAULT_RESULT_FILE")
    run_benchmark "${args[@]}"
}

# 快速测试模式
quick_mode() {
    print_info "运行快速测试模式..."
    create_test_dir "$DEFAULT_DB_DIR"
    
    local args=("--write-batch=1000" "--read-batch=1000" "--key-length=16" "--value-length=128" "--zipf=0.99"
                "--db-dir=$DEFAULT_DB_DIR"
                "--randsrc-filename=$SCRIPT_DIR/randsrc.dat"
                "--result-dir=$DEFAULT_RESULT_DIR"
                "--result-file=$DEFAULT_RESULT_FILE")
    run_benchmark "${args[@]}"
}

# 压力测试模式
stress_mode() {
    print_info "运行压力测试模式 - 测试不同的value-length值..."
    
    local value_lengths=(256 512 1024 2048 4096 8192)
    
    for value_len in "${value_lengths[@]}"; do
        create_test_dir "$DEFAULT_DB_DIR"
        
        print_info "测试 value-length: ${value_len} bytes"
        print_info "数据库目录: $DEFAULT_DB_DIR"
        
        local args=(
            "--write-batch=50000" 
            "--read-batch=50000" 
            "--key-length=32" 
            "--value-length=$value_len" 
            "--zipf=0.99"
            "--db-dir=$DEFAULT_DB_DIR"
            "--randsrc-filename=$SCRIPT_DIR/randsrc.dat"
            "--result-dir=$DEFAULT_RESULT_DIR"
            "--result-file=scaleBenchmark_value_${value_len}.csv"
        )
        
        run_benchmark "${args[@]}"
        
        if [[ "$value_len" != "${value_lengths[-1]}" ]]; then
            print_info "等待5秒后开始下一个测试..."
            sleep 5
        fi
    done
    
    print_success "所有value-length测试完成!"
    
    print_info "测试结果文件:"
    for value_len in "${value_lengths[@]}"; do
        local result_file="$DEFAULT_RESULT_DIR/scaleBenchmark_value_${value_len}.csv"
        if [[ -f "$result_file" ]]; then
            echo "  value-length ${value_len}: $result_file"
        fi
    done
}


# 自定义模式
custom_mode() {
    print_info "运行自定义测试模式..."
    create_test_dir "$DEFAULT_DB_DIR"
    
    run_benchmark "$@"
}

# 运行基准测试
run_benchmark() {
    local args=("$@")
    
    print_info "启动基准测试..."
    echo "=================================================="
    echo "可执行文件: $BENCHMARK_BIN"
    echo "参数: ${args[*]}"
    echo "工作目录: $SCRIPT_DIR"
    echo "=================================================="
    echo ""
    
    cd "$SCRIPT_DIR"
    
    echo "=== 基准测试输出开始 ===" >> "$LOG_FILE"
    "$BENCHMARK_BIN" "${args[@]}" 2>&1 | tee -a "$LOG_FILE"
    echo "=== 基准测试输出结束 ===" >> "$LOG_FILE"
    
    local exit_code=$?
    echo ""
    
    if [[ $exit_code -eq 0 ]]; then
        print_success "基准测试完成!"
        
        local result_file="${DEFAULT_RESULT_FILE}"
        for arg in "${args[@]}"; do
            if [[ "$arg" == --result-file=* ]]; then
                result_file="${arg#--result-file=}"
                break
            fi
        done
        
        local full_result_path="${DEFAULT_RESULT_DIR}${result_file}"
        if [[ -f "$full_result_path" ]]; then
            print_info "结果文件: $full_result_path"
            echo "前10行结果:"
            head -n 10 "$full_result_path"
        elif [[ -f "$result_file" ]]; then
            print_info "结果文件: $result_file"
            echo "前10行结果:"
            head -n 10 "$result_file"
        else
            print_warning "结果文件未找到"
        fi
    else
        print_error "基准测试失败，退出码: $exit_code"
        exit $exit_code
    fi
}

# 显示日志文件信息
show_log_info() {
    echo ""
    echo "=================================================="
    echo "日志文件信息:"
    echo "详细日志: $LOG_FILE"
    echo "日志目录: $LOG_DIR"
    echo "=================================================="
    echo ""
}

# 主函数
main() {
    init_logging

    local mode="${1:-default}"
    shift
    
    case "$mode" in
        "default")
            check_binary
            default_mode
            ;;
        "quick"|"fast")
            check_binary
            quick_mode
            ;;
        "stress"|"pressure")
            check_binary
            stress_mode
            ;;
        "custom")
            check_binary
            custom_mode "$@"
            ;;
        "help"|"-h"|"--help")
            show_usage
            ;;
        *)
            print_error "未知模式: $mode"
            show_usage
            exit 1
            ;;
    esac

    show_log_info
}

# 脚本入口
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
