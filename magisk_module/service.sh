#!/system/bin/sh
MODDIR=${0%/*}
DATA_DIR=/storage/emulated/0/Android/Aether
CONFIG="$DATA_DIR/threads.json"

wait_until_boot() {
    local i=0
    # 等待开机完成
    while [ "$(getprop sys.boot_completed)" != "1" ] && [ $i -lt 60 ]; do
        sleep 1; i=$((i+1))
    done
    # 等待 /storage/emulated/0 挂载 (scoped storage)
    i=0
    while [ ! -d /storage/emulated/0/Android ] && [ $i -lt 60 ]; do
        sleep 1; i=$((i+1))
    done
    mkdir -p "$DATA_DIR" 2>/dev/null
}

wait_until_boot
rm -f "$DATA_DIR/threads_log.txt" 2>/dev/null
rm -f "$DATA_DIR/status.json" 2>/dev/null
pkill "aether-optext" 2>/dev/null
sleep 1

if [ -f "$MODDIR/aether-optext" ]; then
    echo "[Aether] 启动进程..."
    "$MODDIR/aether-optext" -c "$CONFIG" -s 2 &
    echo "[Aether] PID $!"
else
    echo "[Aether] 二进制不存在: $MODDIR/aether-optext"
fi
