use std::{
    env, fs, io::Write, mem,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
    collections::HashSet,
};

#[macro_use]
mod log;
mod config;
mod cpu;
mod process;
mod bpf;

use log::*;
use config::*;
use cpu::*;
use process::*;

// 信号标志:SIGHUP=重载配置,SIGUSR1=强制重扫
static FLAG_RELOAD: AtomicBool = AtomicBool::new(false);
static FLAG_RESCAN: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: i32) {
    match sig {
        libc::SIGHUP => FLAG_RELOAD.store(true, Ordering::Release),
        libc::SIGUSR1 => FLAG_RESCAN.store(true, Ordering::Release),
        _ => {}
    }
}

/// 写入 status.json 供 WebUI 读取
fn write_status(pid: u32, start_sec: u64, topo: &str, big: &str, little: &str,
                rules: usize, ebpf_ok: bool, interval: u64,
                last_procs: usize, last_threads: usize, state: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let uptime = now.saturating_sub(start_sec);
    // 转义反斜杠与引号(核心范围不含特殊字符,此处仅作兜底)
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let json = format!(
        r#"{{"pid":{},"uptime_sec":{},"timestamp":{},"topology":"{}","big":"{}","little":"{}","rules":{},"ebpf":{},"interval":{},"last_bind_procs":{},"last_bind_threads":{},"state":"{}"}}"#,
        pid, uptime, now, esc(topo), esc(big), esc(little),
        rules, ebpf_ok, interval, last_procs, last_threads, esc(state)
    );
    let path = format!("{}/status.json", log::DATA_DIR);
    let _ = fs::write(&path, json.as_bytes());
}

/// 写带 error 字段的错误状态文件,然后删除 pid 文件 (用于退出路径)
fn write_error_state(pid: u32, state: &str, err: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let json = format!(
        r#"{{"pid":{},"uptime_sec":0,"timestamp":{},"topology":"?","big":"?","little":"?","rules":0,"ebpf":false,"interval":0,"last_bind_procs":0,"last_bind_threads":0,"state":"{}","error":"{}"}}"#,
        pid, now, esc(state), esc(err)
    );
    let _ = fs::write(format!("{}/status.json", log::DATA_DIR), json.as_bytes());
    let _ = fs::remove_file(format!("{}/aether.pid", log::DATA_DIR));
}

/// 尝试从模块目录复制默认配置 (按拓扑匹配,失败则用 4+3+1 兜底)
fn deploy_default_config(config_path: &str) -> bool {
    let mod_dir = "/data/adb/modules/aether-optext";
    // 检测拓扑
    let topo = cpu::detect().2;
    let candidates = [
        format!("{}/config/{}.json", mod_dir, topo),
        format!("{}/config/4+3+1.json", mod_dir),
    ];
    for c in &candidates {
        if Path::new(c).exists() {
            if fs::copy(c, config_path).is_ok() {
                info!("已部署默认配置: {}", c);
                return true;
            }
        }
    }
    false
}

fn main() {
    // 进程锁
    std::panic::set_hook(Box::new(|info| {
        let msg = info.payload().downcast_ref::<&str>().copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("?");
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let _ = fs::OpenOptions::new().create(true).append(true).open(log::PATH)
            .map(|mut f| write!(f, "[PANIC] {} at {}\n", msg, loc));
        // 写 panic 状态文件
        let pid = std::process::id();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let json = format!(
            r#"{{"pid":{},"uptime_sec":0,"timestamp":{},"topology":"?","big":"?","little":"?","rules":0,"ebpf":false,"interval":0,"last_bind_procs":0,"last_bind_threads":0,"state":"panic","error":"{}"}}"#,
            pid, now, msg.replace('"', "\\\"").replace('\\', "\\\\")
        );
        let _ = fs::write(format!("{}/status.json", log::DATA_DIR), json.as_bytes());
        // 删除 pid 文件
        let _ = fs::remove_file(format!("{}/aether.pid", log::DATA_DIR));
    }));

    // 重试创建数据目录 (开机时 /storage/emulated/0 可能未就绪)
    let mut dir_ok = false;
    for _ in 0..30 {
        if fs::create_dir_all(log::DATA_DIR).is_ok() {
            // 测试可写
            let test = format!("{}/.w", log::DATA_DIR);
            if fs::write(&test, b"x").is_ok() {
                let _ = fs::remove_file(&test);
                dir_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    fs::write(log::PATH, "").ok();

    let args: Vec<String> = env::args().collect();
    let mut config_path = format!("{}/threads.json", log::DATA_DIR);
    let mut interval = 2u64;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-c" => { i += 1; if i < args.len() { config_path = args[i].clone(); } }
            "-s" => { i += 1; if i < args.len() { interval = args[i].parse().unwrap_or(2); } }
            _ => {}
        }
        i += 1;
    }
    if interval < 1 { interval = 1; }

    // 立即写 PID 文件 + starting 状态,供 WebUI 检测 (在任何可能失败的初始化之前)
    let self_pid = std::process::id();
    let pid_path = format!("{}/aether.pid", log::DATA_DIR);
    let _ = fs::write(&pid_path, format!("{}", self_pid).as_bytes());
    write_status(self_pid, 0, "?", "?", "?", 0, false, interval, 0, 0, "starting");

    if !dir_ok {
        error!("数据目录不可写: {}", log::DATA_DIR);
        write_error_state(self_pid, "dir_error", &format!("数据目录不可写: {}", log::DATA_DIR));
        return;
    }

    // 注册信号处理
    unsafe {
        libc::signal(libc::SIGHUP, on_signal as usize);
        libc::signal(libc::SIGUSR1, on_signal as usize);
        // 忽略 SIGPIPE,避免写已关闭 fd 退出
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    info!("CPU: {} cpuset={}", cpu::present(), Path::new("/dev/cpuset").exists());

    // 配置缺失则部署默认配置
    if !Path::new(&config_path).exists() {
        info!("配置不存在,尝试部署默认配置");
        let _ = deploy_default_config(&config_path);
    }

    let mut cfg = match AppConfig::load(&config_path) {
        Some(c) => c,
        None => {
            error!("配置加载失败: {}", config_path);
            write_error_state(self_pid, "config_error", &format!("配置加载失败: {}", config_path));
            return;
        }
    };
    info!("已加载 {} 条规则", cfg.rules.len());

    // 合并缓存
    let mut all_w = cfg.wild.clone();
    cache::merge(&mut cfg.pkg_set, &mut cfg.rules);
    info!("共 {} 条规则 (含缓存)", cfg.rules.len());

    let (big, little, topo) = cpu::detect();
    info!("拓扑: {} (大核={} 小核={})", topo, big, little);

    // 初始化 cpuset
    process::init_cpuset();

    // 自身限定在小核运行
    if !little.is_empty() && little != "0" {
        let self_pid = std::process::id() as i32;
        let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
        unsafe { libc::CPU_ZERO(&mut set); }
        for part in little.split(',') {
            let part = part.trim();
            if part.is_empty() { continue; }
            if let Some((s, e)) = part.split_once('-') {
                let start: usize = s.parse().unwrap_or(0);
                let end: usize = e.parse().unwrap_or(start);
                for cpu in start..=end { unsafe { libc::CPU_SET(cpu, &mut set); } }
            } else if let Ok(cpu) = part.parse::<usize>() {
                unsafe { libc::CPU_SET(cpu, &mut set); }
            }
        }
        let r = unsafe { libc::sched_setaffinity(self_pid, std::mem::size_of::<libc::cpu_set_t>(), &set) };
        if r != 0 { info!("自身绑核跳过 (errno={})", std::io::Error::last_os_error().raw_os_error().unwrap_or(0)); }
    }

    // eBPF
    let bpf = bpf::probe(cfg.ebpf);
    if bpf.ok { info!("eBPF: 可用"); }

    let _ = fs::create_dir_all(log::DATA_DIR);

    // 启动时自动分配
    let unknown = process::scan_unknown(&cfg.pkg_set, &all_w);
    for (pid, pkg, th) in &unknown {
        info!("新应用: {} ({} 线程)", pkg, th.len());
        cache::save(pkg, &unknown, &big, &little);
    }
    if !unknown.is_empty() {
        cache::merge(&mut cfg.pkg_set, &mut cfg.rules);
        info!("自动分配完成: {} 个", unknown.len());
    }

    let start_sec = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

    let mut lc = 0i32;
    let mut cnt = 0i32;
    let mut cache_scan = 0i32;
    let mut cache: Vec<(i32, String, Vec<(i32, String, String)>)> = Vec::new();
    let rf = AtomicBool::new(false);
    let mut last_procs = 0usize;
    let mut last_threads = 0usize;
    info!("启动");

    write_status(self_pid, start_sec, &topo, &big, &little, cfg.rules.len(), bpf.ok, interval, 0, 0, "running");

    loop {
        // 信号:重载配置
        if FLAG_RELOAD.swap(false, Ordering::AcqRel) {
            match AppConfig::load(&config_path) {
                Some(new_cfg) => {
                    cfg = new_cfg;
                    all_w = cfg.wild.clone();
                    cache::merge(&mut cfg.pkg_set, &mut cfg.rules);
                    info!("配置已重载: {} 条规则 (含缓存)", cfg.rules.len());
                    // 立即重扫
                    cache = process::scan(&cfg.rules, &cfg.pkg_set, &all_w);
                    rf.store(false, Ordering::Release);
                }
                None => error!("重载失败:配置解析错误"),
            }
        }
        // 信号:强制重扫
        if FLAG_RESCAN.swap(false, Ordering::AcqRel) {
            info!("收到强制重扫信号");
            cache = process::scan(&cfg.rules, &cfg.pkg_set, &all_w);
            rf.store(false, Ordering::Release);
        }

        // eBPF map 读取
        if bpf.map_fd >= 0 {
            for pid in bpf::read_map(bpf.map_fd) {
                let cl = fs::read_to_string(format!("/proc/{}/cmdline", pid)).unwrap_or_default();
                let pkg = cl.split('\0').next().unwrap_or("").trim_end_matches('\0').to_string();
                if !pkg.is_empty() && (cfg.pkg_set.contains(&pkg) || all_w.iter().any(|w| fnmatch(w, &pkg))) {
                    info!("eBPF: 新进程 {} ({})", pid, pkg);
                }
            }
        }

        let mut nr = false;

        // 定期扫描新应用
        cache_scan += 1;
        if cache_scan >= 30 {
            cache_scan = 0;
            let u = process::scan_unknown(&cfg.pkg_set, &all_w);
            for (pid, pkg, th) in &u {
                info!("新应用: {} ({} 线程)", pkg, th.len());
                cache::save(pkg, &u, &big, &little);
            }
            if !u.is_empty() {
                cache::merge(&mut cfg.pkg_set, &mut cfg.rules);
                info!("缓存已更新");
            }
        }

        // 进程数检测 (每 5 轮才重扫一次)
        if cache_scan % 5 == 0 {
            let mut si: libc::sysinfo = unsafe { mem::zeroed() };
            if unsafe { libc::sysinfo(&mut si) } != 0 { nr = true; }
            else {
                let cur = si.procs as i32;
                if cur > lc + 10 { nr = true; }
                else if cur > lc { cnt = 0; }
                lc = cur;
            }
            if !nr {
                for (pid, _, _) in &cache {
                    if unsafe { libc::kill(*pid, 0) } != 0 { nr = true; break; }
                }
            }
        }

        if nr {
            cache = process::scan(&cfg.rules, &cfg.pkg_set, &all_w);
            rf.store(false, Ordering::Release);
        }

        cnt -= 1;
        if cnt < 1 {
            (last_procs, last_threads) = process::apply(&cache, &rf);
            if rf.load(Ordering::Acquire) {
                cache = process::scan(&cfg.rules, &cfg.pkg_set, &all_w);
                rf.store(false, Ordering::Release);
            }
            cnt = 5;
        }

        // 写状态文件
        write_status(self_pid, start_sec, &topo, &big, &little,
                     cfg.rules.len(), bpf.ok, interval, last_procs, last_threads, "running");

        std::thread::sleep(Duration::from_secs(interval));
    }
}
