use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
pub const HISTORY: usize = 60;
const DESKTOP_FILE_LIMIT: u64 = 64 * 1024;
static STARTUP_WRITE_ID: AtomicU64 = AtomicU64::new(0);
pub type Series = VecDeque<f32>;
pub fn push(s: &mut Series, n: f32) {
    if s.len() == HISTORY {
        s.pop_front();
    }
    s.push_back(n);
}
fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
fn number(path: impl AsRef<Path>) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn bounded(path: impl AsRef<Path>, limit: u64) -> String {
    let mut s = String::new();
    if let Ok(f) = fs::File::open(path) {
        let _ = f.take(limit).read_to_string(&mut s);
    }
    s
}
fn bounded_desktop_file(path: impl AsRef<Path>) -> Result<String, String> {
    let mut bytes = Vec::new();
    fs::File::open(path).map_err(|e| e.to_string())?.take(DESKTOP_FILE_LIMIT + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > DESKTOP_FILE_LIMIT {
        return Err("The startup entry exceeds the 64 KiB safety limit.".into());
    }
    String::from_utf8(bytes).map_err(|_| "The startup entry is not valid UTF-8.".into())
}
#[derive(Clone, Debug, Default)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub start: u64,
    pub name: String,
    pub command: String,
    pub user: String,
    pub uid: u32,
    pub status: String,
    pub cpu: f32,
    pub total_cpu: f32,
    pub memory: u64,
    pub total_memory: u64,
    pub read_rate: f32,
    pub write_rate: f32,
    pub io_available: bool,
    pub ticks: u64,
    pub threads: u32,
    pub nice: i32,
    pub app: bool,
    pub desktop: Option<Arc<crate::desktop::DesktopApp>>,
}
/// Sum CPU and RSS over the complete snapshot once per sample, independently of UI
/// filtering/folding. Leaves propagate upward without recursion or repeated
/// ancestry walks. Missing/reused parents and self-parent links are roots.
pub fn update_process_totals(processes: &mut [Process]) {
    let by_pid: HashMap<_, _> = processes.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
    let parents: Vec<_> = processes
        .iter()
        .enumerate()
        .map(|(i, p)| by_pid.get(&p.parent).copied().filter(|&parent| parent != i && processes[parent].start <= p.start))
        .collect();
    let mut children = vec![0usize; processes.len()];
    for parent in parents.iter().flatten() {
        children[*parent] += 1;
    }
    let mut ready = Vec::new();
    for (i, p) in processes.iter_mut().enumerate() {
        p.total_cpu = p.cpu;
        p.total_memory = p.memory;
        if children[i] == 0 {
            ready.push(i);
        }
    }
    while let Some(i) = ready.pop() {
        if let Some(parent) = parents[i] {
            processes[parent].total_cpu += processes[i].total_cpu;
            processes[parent].total_memory = processes[parent].total_memory.saturating_add(processes[i].total_memory);
            children[parent] -= 1;
            if children[parent] == 0 {
                ready.push(parent);
            }
        }
    }
    // Only cycles remain. Each member reaches the same processes; sum each
    // attached branch once rather than looping or counting a member twice.
    for i in 0..processes.len() {
        if children[i] == 0 {
            continue;
        }
        let mut cycle = Vec::new();
        let mut next = i;
        let mut total = 0u64;
        let mut total_cpu = 0.0;
        while children[next] != 0 {
            children[next] = 0;
            cycle.push(next);
            total = total.saturating_add(processes[next].total_memory);
            total_cpu += processes[next].total_cpu;
            next = parents[next].expect("remaining nodes have a parent in a cycle");
        }
        for member in cycle {
            processes[member].total_cpu = total_cpu;
            processes[member].total_memory = total;
        }
    }
}
#[derive(Clone, Default)]
pub struct Disk {
    pub name: String,
    pub model: String,
    pub kind: String,
    pub size: u64,
    pub busy: f32,
    pub read_rate: f32,
    pub write_rate: f32,
    pub history: Series,
}
#[derive(Clone, Default)]
pub struct Network {
    pub name: String,
    pub received: f32,
    pub sent: f32,
    pub total_received: u64,
    pub total_sent: u64,
    pub rx_history: Series,
    pub tx_history: Series,
    pub mac: String,
    pub state: String,
    pub physical: bool,
    pub wireless: bool,
}
impl Network {
    pub fn visible(&self, hide_virtual: bool) -> bool {
        !hide_virtual || self.physical
    }
    pub fn kind(&self) -> &'static str {
        if self.wireless {
            "Wi-Fi"
        } else if self.physical {
            "Ethernet"
        } else {
            "Virtual network"
        }
    }
}
#[derive(Clone, Default)]
pub struct Gpu {
    pub name: String,
    pub busy: Option<f32>,
    pub used: Option<u64>,
    pub total: Option<u64>,
    pub temperature: Option<f32>,
    pub history: Series,
    pub(crate) path: PathBuf,
    pub bus: String,
    pub nvidia: bool,
    pub encoder: Option<f32>,
    pub decoder: Option<f32>,
    pub power: Option<f32>,
    pub driver: String,
    pub source: String,
    pub note: String,
    pub memory_history: Series,
    pub encode_history: Series,
    pub decode_history: Series,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub processes: Vec<Process>,
    pub cpu: f32,
    pub load: Option<[f32; 3]>,
    pub cpu_name: String,
    pub cores: usize,
    pub mhz: f32,
    pub cpu_history: Vec<Series>,
    pub cpu_total: Series,
    pub mem_history: Series,
    pub mem_total: u64,
    pub mem_available: u64,
    pub mem_cached: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub uptime: u64,
    pub threads: u64,
    pub disks: Vec<Disk>,
    pub networks: Vec<Network>,
    pub gpus: Vec<Gpu>,
    pub scan_ms: f32,
    pub error: Option<String>,
    pub sampled: u64,
}
impl Snapshot {
    pub fn mem_used(&self) -> u64 {
        self.mem_total.saturating_sub(self.mem_available)
    }
    pub fn mem_percent(&self) -> f32 {
        self.mem_used() as f32 / self.mem_total.max(1) as f32 * 100.0
    }
}
struct Previous {
    start: u64,
    ticks: u64,
    read: u64,
    write: u64,
    name: String,
    command: String,
    user: String,
    uid: u32,
}
pub struct Collector {
    previous: HashMap<u32, Previous>,
    cpu_previous: Vec<(u64, u64)>,
    disk_previous: HashMap<String, (u64, u64, u64)>,
    net_previous: HashMap<String, (u64, u64)>,
    users: HashMap<u32, String>,
    last: Instant,
    page_size: u64,
    clock: f32,
    pub snapshot: Snapshot,
    nvidia: crate::gpu::NvidiaReader,
    desktops: crate::desktop::Catalog,
}
fn cpu_times(text: &str) -> Vec<(u64, u64)> {
    text.lines()
        .take_while(|l| l.starts_with("cpu"))
        .map(|l| {
            let n: Vec<u64> = l.split_whitespace().skip(1).take(8).map(|s| s.parse().unwrap_or(0)).collect();
            (n.iter().sum(), n.get(3).copied().unwrap_or(0) + n.get(4).copied().unwrap_or(0))
        })
        .collect()
}
pub fn parse_stat(s: &str) -> Option<(String, Vec<&str>)> {
    let a = s.find('(')?;
    let b = s.rfind(')')?;
    if b < a {
        return None;
    }
    let fields: Vec<_> = s[b + 1..].split_whitespace().collect();
    if fields.len() < 22 {
        return None;
    }
    Some((s[a + 1..b].to_owned(), fields))
}
fn parse_load(s: &str) -> Option<[f32; 3]> {
    let mut fields = s.split_whitespace();
    let mut values = [0.0; 3];
    for value in &mut values {
        *value = fields.next()?.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0)?;
    }
    Some(values)
}
fn io_counters(pid: u32) -> Option<(u64, u64)> {
    let t = fs::read_to_string(format!("/proc/{pid}/io")).ok()?;
    let get = |key: &str| t.lines().find_map(|l| l.strip_prefix(key)?.trim().parse().ok()).unwrap_or(0);
    Some((get("read_bytes:"), get("write_bytes:")))
}
impl Collector {
    pub fn new() -> Self {
        let users = read("/etc/passwd")
            .lines()
            .filter_map(|l| {
                let p: Vec<_> = l.split(':').collect();
                Some((p.get(2)?.parse().ok()?, p.first()?.to_string()))
            })
            .collect();
        let info = read("/proc/cpuinfo");
        let cpu_name = info
            .lines()
            .find_map(|l| l.strip_prefix("model name")?.split_once(':').map(|x| x.1.trim().to_owned()))
            .unwrap_or_else(|| std::env::consts::ARCH.into());
        let mut snapshot = Snapshot { cpu_name, ..Default::default() };
        if let Ok(entries) = fs::read_dir("/sys/class/drm") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.starts_with("card") || !name[4..].chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let path = e.path().join("device");
                if !path.exists() {
                    continue;
                }
                let vendor = read(path.join("vendor"));
                let brand = match vendor.trim() {
                    "0x1002" => "AMD Radeon",
                    "0x10de" => "NVIDIA",
                    "0x8086" => "Intel Graphics",
                    _ => "GPU",
                };
                snapshot.gpus.push(Gpu {
                    name: format!("{brand} · {name}"),
                    bus: fs::canonicalize(&path)
                        .ok()
                        .and_then(|p| p.file_name().map(|n| crate::gpu::bus_id(&n.to_string_lossy())))
                        .unwrap_or_default(),
                    nvidia: vendor.trim() == "0x10de",
                    source: "sysfs (DRM / hwmon)".into(),
                    path,
                    ..Default::default()
                });
            }
        }
        Self {
            previous: HashMap::new(),
            cpu_previous: Vec::new(),
            disk_previous: HashMap::new(),
            net_previous: HashMap::new(),
            users,
            last: Instant::now(),
            page_size: unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as u64,
            clock: unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f32,
            snapshot,
            nvidia: crate::gpu::NvidiaReader::start(),
            desktops: crate::desktop::Catalog::load(),
        }
    }
    pub fn refresh(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f32().max(0.01);
        self.last = now;
        let raw = read("/proc/stat");
        self.snapshot.error =
            if raw.is_empty() { Some("Cannot read /proc. This application requires Linux with procfs mounted.".into()) } else { None };
        let times = cpu_times(&raw);
        self.snapshot.cores = times.len().saturating_sub(1).max(1);
        self.snapshot.cpu_history.resize_with(self.snapshot.cores, Series::new);
        for (i, &(total, idle)) in times.iter().enumerate() {
            let value = if let Some(&(t, d)) = self.cpu_previous.get(i) {
                let delta = total.saturating_sub(t);
                if delta > 0 {
                    100.0 * (delta.saturating_sub(idle.saturating_sub(d))) as f32 / delta as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };
            if i == 0 {
                self.snapshot.cpu = value;
                push(&mut self.snapshot.cpu_total, value);
            } else {
                push(&mut self.snapshot.cpu_history[i - 1], value);
            }
        }
        self.cpu_previous = times;
        self.snapshot.load = parse_load(&bounded("/proc/loadavg", 256));
        let mem = read("/proc/meminfo");
        let get = |key: &str| mem.lines().find_map(|l| l.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok()).unwrap_or(0) * 1024;
        self.snapshot.mem_total = get("MemTotal:");
        self.snapshot.mem_available = get("MemAvailable:");
        self.snapshot.mem_cached = get("Cached:");
        self.snapshot.swap_total = get("SwapTotal:");
        self.snapshot.swap_free = get("SwapFree:");
        let percent = self.snapshot.mem_percent();
        push(&mut self.snapshot.mem_history, percent);
        self.snapshot.uptime = read("/proc/uptime").split_whitespace().next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) as u64;
        self.snapshot.mhz = number("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").map(|n| n as f32 / 1000.0).unwrap_or(0.0);
        let mut processes = Vec::with_capacity(self.previous.len() + 16);
        let mut next = HashMap::with_capacity(self.previous.len() + 16);
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                    continue;
                };
                let stat = read(entry.path().join("stat"));
                let Some((mut name, f)) = parse_stat(&stat) else {
                    continue;
                };
                if pid == std::process::id() {
                    name = "Task Manager".into();
                }
                let n = |i: usize| f.get(i).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                let ticks = n(11) + n(12);
                let start = n(19);
                let io = io_counters(pid);
                let (rd, wr) = io.unwrap_or_default();
                let old = self.previous.remove(&pid).filter(|p| p.start == start);
                let cpu = old
                    .as_ref()
                    .map(|p| ticks.saturating_sub(p.ticks) as f32 / self.clock / elapsed / self.snapshot.cores as f32 * 100.0)
                    .unwrap_or(0.0)
                    .clamp(0.0, 100.0);
                let rr = old.as_ref().map(|p| rd.saturating_sub(p.read) as f32 / elapsed).unwrap_or(0.0);
                let rw = old.as_ref().map(|p| wr.saturating_sub(p.write) as f32 / elapsed).unwrap_or(0.0);
                let (uid, user, command) = if let Some(p) = old {
                    if p.name == name {
                        (p.uid, p.user, p.command)
                    } else {
                        self.metadata(pid)
                    }
                } else {
                    self.metadata(pid)
                };
                let desktop = self.desktops.find(&name, &command);
                let app = pid == std::process::id() || desktop.is_some();
                processes.push(Process {
                    pid,
                    parent: n(1) as u32,
                    start,
                    name: name.clone(),
                    command: command.clone(),
                    user: user.clone(),
                    uid,
                    status: match f[0] {
                        "R" => "Running",
                        "S" | "I" => "Sleeping",
                        "D" => "Disk wait",
                        "T" | "t" => "Suspended",
                        "Z" => "Zombie",
                        _ => "Unknown",
                    }
                    .into(),
                    cpu,
                    total_cpu: 0.0,
                    memory: n(21) * self.page_size,
                    total_memory: 0,
                    read_rate: rr,
                    write_rate: rw,
                    io_available: io.is_some(),
                    ticks,
                    threads: n(17) as u32,
                    nice: f[16].parse().unwrap_or(0),
                    app,
                    desktop,
                });
                next.insert(pid, Previous { start, ticks, read: rd, write: wr, name, command, user, uid });
            }
        }
        self.previous = next;
        update_process_totals(&mut processes);
        self.snapshot.threads = processes.iter().map(|p| p.threads as u64).sum();
        self.snapshot.processes = processes;
        self.refresh_disks(elapsed);
        self.refresh_networks(elapsed);
        for g in &mut self.snapshot.gpus {
            g.busy = number(g.path.join("gpu_busy_percent")).map(|n| n as f32);
            g.used = number(g.path.join("mem_info_vram_used"));
            g.total = number(g.path.join("mem_info_vram_total"));
            g.temperature = fs::read_dir(g.path.join("hwmon"))
                .ok()
                .and_then(|mut i| i.next())
                .and_then(Result::ok)
                .and_then(|e| number(e.path().join("temp1_input")))
                .map(|n| n as f32 / 1000.0);
            if let Some(v) = g.busy {
                push(&mut g.history, v);
            }
        }
        self.nvidia.merge(&mut self.snapshot.gpus);
        for g in &mut self.snapshot.gpus {
            if let Some((used, total)) = g.used.zip(g.total) {
                push(&mut g.memory_history, used as f32 / total.max(1) as f32 * 100.0);
            }
        }
        self.snapshot.scan_ms = now.elapsed().as_secs_f32() * 1000.0;
        self.snapshot.sampled += 1;
    }
    fn metadata(&self, pid: u32) -> (u32, String, String) {
        let uid = read(format!("/proc/{pid}/status"))
            .lines()
            .find_map(|l| l.strip_prefix("Uid:")?.split_whitespace().next()?.parse::<u32>().ok())
            .unwrap_or(u32::MAX);
        let user = self.users.get(&uid).cloned().unwrap_or_else(|| uid.to_string());
        let command = bounded(format!("/proc/{pid}/cmdline"), 4096).replace('\0', " ");
        (uid, user, command.trim().into())
    }
    fn refresh_disks(&mut self, dt: f32) {
        let mut disks = Vec::new();
        for l in read("/proc/diskstats").lines() {
            let f: Vec<_> = l.split_whitespace().collect();
            if f.len() < 14 {
                continue;
            }
            let name = f[2];
            let path = PathBuf::from(format!("/sys/class/block/{name}"));
            if name.starts_with("loop") || name.starts_with("ram") || path.join("partition").exists() {
                continue;
            }
            let n = |i: usize| f[i].parse::<u64>().unwrap_or(0);
            let (rd, wr, busy) = (n(5) * 512, n(9) * 512, n(12));
            let prev = self.disk_previous.insert(name.into(), (rd, wr, busy));
            let (rr, rw, usage) = prev
                .map(|(r, w, b)| {
                    (rd.saturating_sub(r) as f32 / dt, wr.saturating_sub(w) as f32 / dt, (busy.saturating_sub(b) as f32 / (dt * 10.0)).min(100.0))
                })
                .unwrap_or((0.0, 0.0, 0.0));
            let old = self.snapshot.disks.iter().find(|d| d.name == name);
            let mut hist = old.map(|d| d.history.clone()).unwrap_or_default();
            push(&mut hist, usage);
            disks.push(Disk {
                name: name.into(),
                model: read(path.join("device/model")).trim().into(),
                kind: if number(path.join("queue/rotational")) == Some(1) { "HDD" } else { "SSD / virtual" }.into(),
                size: number(path.join("size")).unwrap_or(0) * 512,
                busy: usage,
                read_rate: rr,
                write_rate: rw,
                history: hist,
            });
        }
        self.snapshot.disks = disks;
    }
    fn refresh_networks(&mut self, dt: f32) {
        let mut nets = Vec::new();
        for l in read("/proc/net/dev").lines().skip(2) {
            let Some((name, values)) = l.split_once(':') else {
                continue;
            };
            let name = name.trim();
            if name == "lo" {
                continue;
            }
            let f: Vec<_> = values.split_whitespace().collect();
            if f.len() < 9 {
                continue;
            }
            let rx = f[0].parse::<u64>().unwrap_or(0);
            let tx = f[8].parse::<u64>().unwrap_or(0);
            let prev = self.net_previous.insert(name.into(), (rx, tx));
            let (rr, tr) = prev.map(|(r, t)| (rx.saturating_sub(r) as f32 / dt, tx.saturating_sub(t) as f32 / dt)).unwrap_or((0.0, 0.0));
            let old = self.snapshot.networks.iter().find(|n| n.name == name);
            let mut rh = old.map(|n| n.rx_history.clone()).unwrap_or_default();
            let mut th = old.map(|n| n.tx_history.clone()).unwrap_or_default();
            push(&mut rh, rr);
            push(&mut th, tr);
            nets.push(Network {
                name: name.into(),
                received: rr,
                sent: tr,
                total_received: rx,
                total_sent: tx,
                rx_history: rh,
                tx_history: th,
                mac: read(format!("/sys/class/net/{name}/address")).trim().into(),
                state: read(format!("/sys/class/net/{name}/operstate")).trim().into(),
                physical: Path::new(&format!("/sys/class/net/{name}/device")).exists() && number(format!("/sys/class/net/{name}/type")) == Some(1),
                wireless: Path::new(&format!("/sys/class/net/{name}/wireless")).exists(),
            });
        }
        self.snapshot.networks = nets;
    }
}
pub struct Monitor {
    pub latest: Arc<Mutex<Option<Arc<Snapshot>>>>,
    pub interval: Arc<AtomicU64>,
    stopping: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Monitor {
    pub fn demo() -> Self {
        Self {
            latest: Arc::new(Mutex::new(Some(Arc::new(crate::demo::snapshot())))),
            interval: Arc::new(AtomicU64::new(1000)),
            stopping: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }
    #[cfg(test)]
    pub fn idle() -> Self {
        Self {
            latest: Arc::new(Mutex::new(None)),
            interval: Arc::new(AtomicU64::new(1000)),
            stopping: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    pub fn start(ctx: eframe::egui::Context) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let interval = Arc::new(AtomicU64::new(1000));
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let (l, i) = (latest.clone(), interval.clone());
        let worker = thread::spawn(move || {
            let mut c = Collector::new();
            while !stop.load(Ordering::Relaxed) {
                let ms = i.load(Ordering::Relaxed);
                if ms > 0 || c.snapshot.sampled == 0 {
                    c.refresh();
                    if let Ok(mut slot) = l.lock() {
                        *slot = Some(Arc::new(c.snapshot.clone()));
                    }
                    ctx.request_repaint();
                }
                thread::park_timeout(Duration::from_millis(if ms == 0 { 200 } else { ms }));
            }
        });
        Self { latest, interval, stopping, thread: Some(worker) }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(worker) = self.thread.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}
pub fn signal_process(pid: u32, start: u64, signal: i32) -> Result<(), String> {
    if pid <= 1 || pid == std::process::id() {
        return Err("This process is protected.".into());
    }
    // A pidfd pins process identity even if it exits and Linux reuses its PID.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    let open_error = io::Error::last_os_error();
    let raw = read(format!("/proc/{pid}/stat"));
    let valid = parse_stat(&raw).is_some_and(|(_, f)| f[19].parse::<u64>().ok() == Some(start));
    if !valid {
        if fd >= 0 {
            unsafe {
                libc::close(fd);
            }
        }
        return Err("The process exited or changed. Select it again.".into());
    }
    if fd < 0 {
        return Err(format!("Cannot safely access this process: {}", open_error));
    }
    let result = unsafe { libc::syscall(libc::SYS_pidfd_send_signal, fd, signal, std::ptr::null::<libc::siginfo_t>(), 0) };
    let error = io::Error::last_os_error();
    unsafe {
        libc::close(fd);
    }
    if result == 0 {
        Ok(())
    } else {
        Err(error.to_string())
    }
}
pub fn efficiency(pid: u32, start: u64) -> Result<(), String> {
    // Validate identity without delivering a signal.
    signal_process(pid, start, 0)?;
    let raw = read(format!("/proc/{pid}/stat"));
    if parse_stat(&raw).is_some_and(|(_, f)| f[16].parse::<i32>().is_ok_and(|nice| nice >= 10)) {
        return Ok(());
    }
    if unsafe { libc::setpriority(libc::PRIO_PROCESS, pid, 10) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().to_string())
    }
}
#[derive(Clone, Default)]
pub struct Service {
    pub name: String,
    pub description: String,
    pub state: String,
    pub scope: String,
}
pub fn services() -> (Vec<Service>, String) {
    let mut result = Vec::new();
    let mut errors = Vec::new();
    for user in [false, true] {
        let mut c = Command::new("systemctl");
        if user {
            c.arg("--user");
        }
        let out = c.args(["list-units", "--type=service", "--all", "--no-pager", "--plain", "--no-legend"]).env("LC_ALL", "C").output();
        match out {
            Ok(o) if o.status.success() => {
                for l in String::from_utf8_lossy(&o.stdout).lines() {
                    let f: Vec<_> = l.split_whitespace().collect();
                    if f.len() >= 4 {
                        result.push(Service {
                            name: f[0].into(),
                            description: f[4..].join(" "),
                            state: f[2].into(),
                            scope: if user { "User" } else { "System" }.into(),
                        });
                    }
                }
            }
            Ok(o) => errors.push(String::from_utf8_lossy(&o.stderr).trim().to_owned()),
            Err(e) => errors.push(e.to_string()),
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    (result, errors.join("\n"))
}
pub fn service_action(s: &Service, action: &str) -> Result<(), String> {
    let mut c = Command::new("systemctl");
    if s.scope == "User" {
        c.arg("--user");
    }
    let o = c.args(["--no-ask-password", action, "--", &s.name]).env("LC_ALL", "C").output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().into())
    }
}
#[derive(Clone)]
pub struct Startup {
    pub name: String,
    pub command: String,
    pub enabled: bool,
    pub file: String,
    pub source: String,
    pub filename: String,
}
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
}
pub fn startups() -> Vec<Startup> {
    let mut map = HashMap::new();
    let dirs = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".into());
    let mut paths: Vec<_> = dirs.split(':').rev().map(|d| PathBuf::from(d).join("autostart")).collect();
    paths.push(config_dir().join("autostart"));
    for path in paths {
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for e in entries.flatten() {
            if e.path().extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }
            // Desktop entries are user/package supplied input. Do not let one
            // oversized file consume unbounded memory during catalog loading.
            let text = bounded_desktop_file(e.path()).unwrap_or_default();
            let get = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)).unwrap_or("").to_string();
            let name = get("Name=");
            if name.is_empty() {
                continue;
            }
            let filename = e.file_name().to_string_lossy().into_owned();
            map.insert(
                filename.clone(),
                Startup {
                    name,
                    command: get("Exec="),
                    enabled: get("Hidden=") != "true" && get("X-GNOME-Autostart-enabled=") != "false",
                    file: e.path().to_string_lossy().into_owned(),
                    source: if e.path().starts_with(config_dir()) { "User" } else { "System" }.into(),
                    filename,
                },
            );
        }
    }
    let mut v: Vec<_> = map.into_values().collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}
pub fn toggle_startup(s: &Startup) -> Result<(), String> {
    if Path::new(&s.filename).file_name().and_then(|name| name.to_str()) != Some(s.filename.as_str()) || !s.filename.ends_with(".desktop") {
        return Err("Invalid startup entry name.".into());
    }
    let text = bounded_desktop_file(&s.file)?;
    let mut output = String::new();
    let mut in_entry = false;
    let mut found = false;
    for l in text.lines() {
        if l.starts_with('[') {
            if in_entry && !found {
                output.push_str(&format!("Hidden={}\n", s.enabled));
                found = true;
            }
            in_entry = l.trim() == "[Desktop Entry]";
        }
        if in_entry && (l.starts_with("Hidden=") || l.starts_with("X-GNOME-Autostart-enabled=")) {
            continue;
        }
        output.push_str(l);
        output.push('\n');
    }
    if !found {
        output.push_str(&format!("Hidden={}\n", s.enabled));
    }
    let dir = config_dir().join("autostart");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    atomic_write_startup(&dir.join(&s.filename), output.as_bytes()).map_err(|e| e.to_string())
}

/// Replace a startup entry atomically. rename replaces a symlink itself instead
/// of following it, preventing an existing destination symlink from redirecting
/// this write to another user file.
fn atomic_write_startup(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "startup path has no parent"))?;
    for _ in 0..16 {
        let id = STARTUP_WRITE_ID.fetch_add(1, Ordering::Relaxed);
        let temporary = dir.join(format!(".linux-task-manager-{}-{id}.tmp", std::process::id()));
        let mut file = match fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = file.write_all(contents).and_then(|()| file.sync_all()).and_then(|()| fs::rename(&temporary, path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "could not allocate a temporary startup file"))
}
pub fn launch(command: &str) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("Enter a command to run.".into());
    }
    Command::new("sh")
        .args(["-c", command])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut child| {
            thread::spawn(move || {
                let _ = child.wait();
            });
        })
        .map_err(|e| e.to_string())
}
pub fn bytes(n: u64) -> String {
    if n >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", n as f64 / 1073741824.0)
    } else if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1048576.0)
    } else {
        format!("{:.0} KB", n as f64 / 1024.0)
    }
}
pub fn rate(n: f32) -> String {
    format!("{}/s", bytes(n.max(0.0) as u64))
}
pub fn bits(n: f32) -> String {
    let b = n * 8.0;
    if b >= 1e6 {
        format!("{:.1} Mbps", b / 1e6)
    } else {
        format!("{:.1} Kbps", b / 1000.0)
    }
}
pub fn self_test() {
    let mut c = Collector::new();
    c.refresh();
    assert!(c.snapshot.mem_total > 0);
    assert!(!c.snapshot.processes.is_empty());
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    thread::sleep(Duration::from_millis(50));
    c.refresh();
    let p = c.snapshot.processes.iter().find(|p| p.pid == child.id()).unwrap();
    assert!(signal_process(p.pid, p.start + 1, 0).is_err());
    assert!(signal_process(std::process::id(), 0, libc::SIGTERM).is_err());
    signal_process(p.pid, p.start, libc::SIGTERM).unwrap();
    let exit = child.wait().unwrap();
    assert!(!exit.success());
    println!(
        "PASS: procfs, memory, CPU, process identity, protected processes and termination of disposable child; {} processes, {} cores, {:.1} ms scan",
        c.snapshot.processes.len(),
        c.snapshot.cores,
        c.snapshot.scan_ms
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_startup_input_is_bounded() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("task-manager-desktop-{}-{unique}", std::process::id()));
        fs::write(&path, vec![b'a'; DESKTOP_FILE_LIMIT as usize]).unwrap();
        assert_eq!(bounded_desktop_file(&path).unwrap().len(), DESKTOP_FILE_LIMIT as usize);
        fs::write(&path, vec![b'a'; DESKTOP_FILE_LIMIT as usize + 1]).unwrap();
        assert!(bounded_desktop_file(&path).is_err(), "oversized startup metadata must be rejected");
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn startup_replacement_does_not_follow_symlink_destinations() {
        use std::os::unix::fs::symlink;

        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("task-manager-startup-{}-{unique}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let target = dir.join("important-user-file");
        let destination = dir.join("entry.desktop");
        fs::write(&target, "preserve me").unwrap();
        symlink(&target, &destination).unwrap();

        atomic_write_startup(&destination, b"[Desktop Entry]\nHidden=true\n").unwrap();

        assert_eq!(fs::read_to_string(target).unwrap(), "preserve me");
        assert!(!fs::symlink_metadata(&destination).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(destination).unwrap(), "[Desktop Entry]\nHidden=true\n");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn load_averages_validate_all_three_intervals() {
        assert_eq!(parse_load("1.25 0.84 0.65 2/400 1200\n"), Some([1.25, 0.84, 0.65]));
        assert_eq!(parse_load("0 0 0"), Some([0.0; 3]));
        for value in ["", "1.0 2.0", "1.0 broken 2.0", "-1 2 3", "1 NaN 3", "1 2 inf"] {
            assert_eq!(parse_load(value), None, "unavailable or invalid load must not become a false zero: {value}");
        }
    }
    fn memory_process(pid: u32, parent: u32, memory: u64) -> Process {
        Process { pid, parent, memory, ..Default::default() }
    }
    #[test]
    fn total_memory_includes_every_descendant_and_refreshes() {
        // Deliberately unordered, with separate app and background branches.
        let mut ps =
            vec![memory_process(3, 2, 30), memory_process(1, 0, 10), memory_process(4, 1, 40), memory_process(2, 1, 20), memory_process(5, 99, 50)];
        ps[1].app = true;
        update_process_totals(&mut ps);
        assert_eq!(ps.iter().map(|p| p.total_memory).collect::<Vec<_>>(), [30, 100, 40, 50, 50]);
        assert_eq!(ps[1].memory, 10, "own RSS is preserved");
        ps.remove(0);
        ps[2].memory = 25;
        update_process_totals(&mut ps);
        assert_eq!(ps.iter().map(|p| p.total_memory).collect::<Vec<_>>(), [75, 40, 25, 50]);
    }
    #[test]
    fn total_memory_handles_cycles_self_parents_and_overflow() {
        let mut ps = vec![memory_process(1, 2, 10), memory_process(2, 1, 20), memory_process(3, 2, 30), memory_process(4, 4, 40)];
        update_process_totals(&mut ps);
        assert_eq!(ps.iter().map(|p| p.total_memory).collect::<Vec<_>>(), [60, 60, 30, 40]);
        ps = vec![memory_process(1, 0, u64::MAX), memory_process(2, 1, 1)];
        update_process_totals(&mut ps);
        assert_eq!(ps[0].total_memory, u64::MAX);
        ps[0].memory = 10;
        ps[0].start = 100;
        ps[1].start = 50;
        update_process_totals(&mut ps);
        assert_eq!(ps[0].total_memory, 10, "a reused newer parent is not attached");
        assert_eq!(ps[1].total_memory, 1);
    }
    #[test]
    fn total_cpu_includes_descendants_and_refreshes_without_changing_own_cpu() {
        let process = |pid, parent, cpu| Process { pid, parent, cpu, ..Default::default() };
        let mut ps = vec![process(3, 2, 3.5), process(1, 0, 1.0), process(4, 1, 4.0), process(2, 1, 2.0), process(5, 99, 5.0)];
        update_process_totals(&mut ps);
        assert_eq!(ps.iter().map(|p| p.total_cpu).collect::<Vec<_>>(), [3.5, 10.5, 4.0, 5.5, 5.0]);
        assert_eq!(ps[1].cpu, 1.0);
        ps.remove(0);
        ps[2].cpu = 0.5;
        update_process_totals(&mut ps);
        assert_eq!(ps.iter().map(|p| p.total_cpu).collect::<Vec<_>>(), [5.5, 4.0, 0.5, 5.0]);
        ps[0].start = 100;
        ps[2].start = 50;
        update_process_totals(&mut ps);
        assert_eq!(ps[0].total_cpu, 1.0, "reused parent is not included");
        let mut cycle = vec![process(1, 2, 1.0), process(2, 1, 2.0), process(3, 2, 3.0), process(4, 4, 4.0)];
        update_process_totals(&mut cycle);
        assert_eq!(cycle.iter().map(|p| p.total_cpu).collect::<Vec<_>>(), [6.0, 6.0, 3.0, 4.0]);
    }
    #[test]
    fn total_memory_handles_deep_trees_without_recursion() {
        let mut ps: Vec<_> = (1..=20_000).map(|pid| Process { cpu: 0.001, ..memory_process(pid, pid - 1, 1) }).collect();
        update_process_totals(&mut ps);
        assert_eq!(ps[0].total_memory, 20_000);
        assert_eq!(ps[19_999].total_memory, 1);
        assert!((ps[0].total_cpu - 20.0).abs() < 0.01);
        assert_eq!(ps[19_999].total_cpu, 0.001);
        update_process_totals(&mut []);
    }
    #[test]
    fn stat_handles_parentheses() {
        let text = "123 (name with ) brackets) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22";
        let (n, f) = parse_stat(text).unwrap();
        assert_eq!(n, "name with ) brackets");
        assert_eq!(f[0], "S");
        assert_eq!(f[19], "19");
    }
    #[test]
    fn history_is_bounded() {
        let mut h = Series::new();
        for i in 0..1000 {
            push(&mut h, i as f32);
        }
        assert_eq!(h.len(), 60);
        assert_eq!(h[0], 940.0);
    }
    #[test]
    fn cpu_does_not_double_count_guest() {
        assert_eq!(cpu_times("cpu  10 0 10 80 0 0 0 0 5 0\nintr 0"), vec![(100, 80)]);
    }
}
