#!/system/bin/sh
# action.sh — Magisk 模块操作按钮:启动 WebUI 主机应用
# 用于没有原生 WebUI 的 Magisk,通过安装 KSU WebUI Standalone 或 WebUI X 来加载本模块的 webroot
MODDIR=${0%/*}

# 检查并启动 WebUI 主机应用 (按优先级)
# 1. KSU WebUI Standalone (io.github.a13e300.ksuwebui)
# 2. WebUI X (com.dergoogler.mmrl.wx)
# 3. WebUI X Portable (com.dergoogler.mmrl.wx.portable)

launch_webui_host() {
    local pkg="$1"
    if pm path "$pkg" 2>/dev/null | grep -q "package:"; then
        # KSU WebUI Standalone 用 id extra,WebUI X 用 moduleId extra
        am start -n "$pkg/.WebUIActivity" -e id "aether-optext" -e moduleId "aether-optext" 2>/dev/null
        return $?
    fi
    return 1
}

# 尝试已知的 WebUI 主机应用
for pkg in \
    "io.github.a13e300.ksuwebui" \
    "com.dergoogler.mmrl.wx" \
    "com.dergoogler.mmrl.wx.portable" \
    "com.dergoogler.mmrl"; do
    if launch_webui_host "$pkg"; then
        echo "[Aether] 已启动 WebUI 主机: $pkg"
        exit 0
    fi
done

# 没有任何 WebUI 主机应用 — 提示用户
echo "[Aether] 未找到 WebUI 主机应用"
echo ""
echo "Magisk 不支持原生 WebUI,需要安装以下任一应用:"
echo "  1. KSU WebUI Standalone (io.github.a13e300.ksuwebui)"
echo "  2. WebUI X (com.dergoogler.mmrl.wx)"
echo ""
echo "安装后再次点击模块的「操作」按钮即可打开 WebUI"
echo ""
echo "或者,使用 KernelSU / APatch / MMRL 管理器直接打开 WebUI"

# 返回非 0 表示失败 (Magisk 会显示脚本的 stderr)
exit 1
