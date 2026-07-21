#!/system/bin/sh
MODDIR=${0%/*}
DATA_DIR=/storage/emulated/0/Android/Aether
CONFIG="$DATA_DIR/threads.json"
PID_FILE="$DATA_DIR/aether.pid"
SVC_LOG="$DATA_DIR/service.log"

# 拓扑检测 (与 customize.sh 一致)
detect_topology() {
    local counts=""
    for policy in /sys/devices/system/cpu/cpufreq/policy[0-9]*; do
        [ -d "$policy" ] || continue
        local cpus=$(cat "$policy/related_cpus" 2>/dev/null)
        [ -z "$cpus" ] && continue
        local count=0
        for c in $(echo "$cpus" | tr ',' ' ' | tr '-' ' '); do
            count=$((count + 1))
        done
        if echo "$cpus" | grep -q '-'; then
            local start=$(echo "$cpus" | cut -d'-' -f1)
            local end=$(echo "$cpus" | cut -d'-' -f2)
            count=$((end - start + 1))
        fi
        counts="$counts $count"
    done
    local topo=$(echo "$counts" | xargs | tr ' ' '\n' | tr '\n' '+' | sed 's/^+//;s/+$//')
    [ -z "$topo" ] && topo="4+3+1"
    echo "$topo"
}

# 等待 /storage/emulated/0/Android 出现并可写
wait_storage() {
    local i=0
    while [ "$(getprop sys.boot_completed)" != "1" ] && [ $i -lt 90 ]; do
        sleep 1; i=$((i+1))
    done
    i=0
    while [ ! -d /storage/emulated/0/Android ] && [ $i -lt 90 ]; do
        sleep 1; i=$((i+1))
    done
    # 测试可写性 (FUSE 可能延迟挂载)
    i=0
    while [ $i -lt 30 ]; do
        mkdir -p "$DATA_DIR" 2>/dev/null
        if (echo ok > "$DATA_DIR/.write_test" 2>/dev/null) && [ "$(cat "$DATA_DIR/.write_test" 2>/dev/null)" = "ok" ]; then
            rm -f "$DATA_DIR/.write_test" 2>/dev/null
            return 0
        fi
        sleep 1; i=$((i+1))
    done
    return 1
}

# 所有输出重定向到 service.log (WebUI 可读)
exec > "$SVC_LOG" 2>&1 2>/dev/null
echo "[Aether] service.sh 启动于 $(date 2>/dev/null)"
echo "[Aether] MODDIR=$MODDIR"

wait_storage || echo "[Aether] 警告: 存储未就绪,仍尝试启动"

mkdir -p "$DATA_DIR" 2>/dev/null

# 配置缺失时自动部署默认配置
if [ ! -f "$CONFIG" ]; then
    TOPOLOGY=$(detect_topology)
    if [ -f "$MODDIR/config/${TOPOLOGY}.json" ]; then
        cp "$MODDIR/config/${TOPOLOGY}.json" "$CONFIG" 2>/dev/null
        echo "[Aether] 已部署默认配置 ($TOPOLOGY)"
    elif [ -f "$MODDIR/config/4+3+1.json" ]; then
        cp "$MODDIR/config/4+3+1.json" "$CONFIG" 2>/dev/null
        echo "[Aether] 已部署默认配置 (fallback 4+3+1)"
    fi
fi

rm -f "$DATA_DIR/threads_log.txt" 2>/dev/null
rm -f "$DATA_DIR/status.json" 2>/dev/null
rm -f "$PID_FILE" 2>/dev/null
pkill "aether-optext" 2>/dev/null
sleep 1

if [ ! -f "$MODDIR/aether-optext" ]; then
    echo "[Aether] 错误: 二进制不存在: $MODDIR/aether-optext"
    exit 1
fi

# 验证可执行权限
chmod 0755 "$MODDIR/aether-optext" 2>/dev/null
if [ ! -x "$MODDIR/aether-optext" ]; then
    echo "[Aether] 错误: 二进制不可执行"
    exit 1
fi

echo "[Aether] 启动进程..."
# 用 setsid 完全脱离父进程,避免 service.sh 退出时被 SIGHUP
setsid "$MODDIR/aether-optext" -c "$CONFIG" -s 2 >/dev/null 2>&1 &
APID=$!
echo "[Aether] PID $APID"

# 等待 2 秒,检查进程是否存活 (捕获立即退出的情况)
sleep 2
if kill -0 $APID 2>/dev/null; then
    echo "[Aether] 启动成功,进程存活"
else
    echo "[Aether] 错误: 进程启动后立即退出"
    echo "[Aether] --- threads_log.txt 最后 20 行 ---"
    tail -n 20 "$DATA_DIR/threads_log.txt" 2>/dev/null
    echo "[Aether] --- end ---"
fi
