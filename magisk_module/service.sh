#!/system/bin/sh
MODDIR=${0%/*}
CONFIG="/sdcard/Android/Aether/threads.json"

wait_until_login() {
    local i=0
    while [ "$(getprop sys.boot_completed)" != "1" ] && [ $i -lt 60 ]; do
        sleep 1; i=$((i+1))
    done
    i=0
    while [ $i -lt 30 ]; do
        mkdir -p "/sdcard/Android/Aether" 2>/dev/null && break
        sleep 1; i=$((i+1))
    done
}

wait_until_login
rm -f /sdcard/Android/Aether/threads_log.txt 2>/dev/null
pkill "aether-optext" 2>/dev/null
sleep 1

if [ -f "$MODDIR/aether-optext" ]; then
    echo "[Aether] 启动进程..."
    "$MODDIR/aether-optext" -c "$CONFIG" -s 2 &
    echo "[Aether] PID $!"
else
    echo "[Aether] 二进制不存在: $MODDIR/aether-optext"
fi
