#!/system/bin/sh
# Aether OptExt — Magisk/KernelSU 安装脚本

set_perm_recursive $MODPATH 0 0 0755 0644
set_perm $MODPATH/aether-optext 0 0 0755
[ -d "$MODPATH/webroot" ] && set_perm_recursive $MODPATH/webroot 0 0 0755 0644

# 数据目录 (持久化,模块更新不覆盖)
DATA_DIR=/data/adb/aether
mkdir -p "$DATA_DIR" 2>/dev/null

# 清除旧缓存 (旧位置 + 新位置)
rm -f /sdcard/Android/Aether/threads_cache 2>/dev/null
rm -f "$DATA_DIR/threads_cache" 2>/dev/null

# 迁移旧配置 (从 /sdcard/Android/Aether -> /data/adb/aether)
if [ -f /sdcard/Android/Aether/threads.json ] && [ ! -f "$DATA_DIR/threads.json" ]; then
    cp /sdcard/Android/Aether/threads.json "$DATA_DIR/threads.json" 2>/dev/null
    ui_print "- 已迁移旧配置文件"
fi

# 按频率检测 CPU 拓扑
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
    [ -z "$topo" ] && topo="unknown"
    echo "$topo"
}

TOPOLOGY=$(detect_topology)
ui_print "- CPU: $TOPOLOGY"

if [ -f "$MODPATH/config/${TOPOLOGY}.json" ]; then
    # 首次安装或用户未自定义时覆盖;否则保留现有配置
    if [ ! -f "$DATA_DIR/threads.json" ]; then
        cp "$MODPATH/config/${TOPOLOGY}.json" "$DATA_DIR/threads.json" 2>/dev/null
        ui_print "- 配置已部署"
    else
        ui_print "- 保留现有配置 (如需重置请删除 $DATA_DIR/threads.json)"
    fi
else
    [ -f "$DATA_DIR/threads.json" ] || ui_print "- 请手动配置 threads.json"
fi

ui_print "- Aether OptExt 安装完成"
ui_print "- 数据目录: $DATA_DIR"
ui_print "- 日志: $DATA_DIR/threads_log.txt"
ui_print "- WebUI: 在 KernelSU/APatch/Magisk Delta 管理器中打开"
