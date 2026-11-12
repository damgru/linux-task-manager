//! Public-safe fixtures. No procfs, desktop catalog, services or user files are read.
use crate::model::*;

pub fn enabled() -> bool {
    std::env::args().any(|arg| arg == "--demo")
}

fn series(base: f32, amplitude: f32, offset: usize) -> Series {
    (0..HISTORY).map(|i| base + ((i + offset) as f32 * 0.63).sin().abs() * amplitude).collect()
}

pub fn snapshot() -> Snapshot {
    let gib = 1024 * 1024 * 1024;
    let rows = [
        (200, 0, "Orbit Browser", true, 612, 4.2),
        (201, 200, "Orbit renderer", false, 180, 1.3),
        (202, 200, "Orbit network", false, 42, 0.4),
        (300, 0, "Paper Notes", true, 84, 0.2),
        (400, 0, "Meadow Music", true, 126, 0.6),
        (500, 0, "Task Manager", true, 48, 0.3),
        (600, 0, "demo-session", false, 32, 0.1),
        (601, 600, "demo-compositor", false, 96, 1.1),
        (602, 600, "demo-audio", false, 18, 0.2),
        (603, 600, "demo-notifications", false, 12, 0.0),
        (700, 0, "demo-service-host", false, 24, 0.1),
        (701, 700, "demo-network", false, 16, 0.2),
        (702, 700, "demo-scheduler", false, 8, 0.0),
        (703, 700, "demo-device-manager", false, 14, 0.0),
        (800, 0, "demo-indexer", false, 38, 0.4),
        (801, 800, "demo-index-worker", false, 22, 0.2),
    ];
    Snapshot {
        processes: rows
            .into_iter()
            .map(|(pid, parent, name, app, mb, cpu)| Process {
                pid,
                parent,
                start: pid as u64,
                name: name.into(),
                command: format!("/opt/demo/{name}"),
                user: "demo".into(),
                uid: 1000,
                status: "Sleeping".into(),
                cpu,
                memory: mb * 1024 * 1024,
                read_rate: 8192.0,
                write_rate: 4096.0,
                io_available: true,
                ticks: 250,
                threads: 4,
                app,
                ..Default::default()
            })
            .collect(),
        cpu: 12.0,
        cpu_name: "Demo 8-Core Processor".into(),
        cores: 8,
        mhz: 3600.0,
        cpu_history: (0..8).map(|i| series(3.0, 18.0, i * 7)).collect(),
        cpu_total: series(4.0, 14.0, 0),
        mem_history: series(32.0, 3.0, 0),
        mem_total: 16 * gib,
        mem_available: 11 * gib,
        mem_cached: 2 * gib,
        swap_total: 4 * gib,
        swap_free: 4 * gib,
        uptime: 45296,
        threads: 384,
        disks: vec![Disk {
            name: "nvme0n1".into(),
            model: "Demo NVMe SSD".into(),
            kind: "SSD (NVMe)".into(),
            size: 512 * gib,
            busy: 4.0,
            read_rate: 2_400_000.0,
            write_rate: 640_000.0,
            history: series(1.0, 8.0, 0),
        }],
        networks: vec![Network {
            name: "demo-ethernet".into(),
            received: 1_250_000.0,
            sent: 240_000.0,
            total_received: 256 * 1024 * 1024,
            total_sent: 48 * 1024 * 1024,
            rx_history: series(80_000.0, 1_200_000.0, 0),
            tx_history: series(10_000.0, 240_000.0, 8),
            mac: "02:00:00:00:00:01".into(),
            state: "up".into(),
            physical: true,
            wireless: false,
        }],
        gpus: vec![Gpu {
            name: "Demo Graphics 8 GB".into(),
            busy: Some(9.0),
            used: Some(gib),
            total: Some(8 * gib),
            temperature: Some(42.0),
            history: series(2.0, 12.0, 0),
            bus: "0000:01:00.0".into(),
            encoder: Some(3.0),
            decoder: Some(0.0),
            power: Some(24.0),
            driver: "Demo driver".into(),
            source: "Fictional demonstration data".into(),
            memory_history: series(10.0, 3.0, 0),
            encode_history: series(0.0, 4.0, 4),
            decode_history: series(0.0, 1.0, 0),
            ..Default::default()
        }],
        scan_ms: 0.0,
        error: None,
        sampled: 1,
    }
}

pub fn services() -> Vec<Service> {
    ["audio", "desktop", "network", "notifications", "scheduler", "storage", "updates"]
        .into_iter()
        .map(|name| Service {
            name: format!("demo-{name}.service"),
            description: format!("Demo {name} service"),
            state: "running".into(),
            scope: "system".into(),
        })
        .collect()
}

pub fn startups() -> Vec<Startup> {
    ["Meadow Music", "Paper Notes", "Demo notifications"]
        .into_iter()
        .map(|name| Startup {
            name: name.into(),
            command: format!("/opt/demo/{name}"),
            enabled: true,
            file: String::new(),
            source: "Demo".into(),
            filename: format!("demo-{name}.desktop"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixtures_are_complete_and_monitor_never_starts_a_collector() {
        let monitor = Monitor::demo();
        let guard = monitor.latest.lock().unwrap();
        let data = guard.as_ref().unwrap();
        assert_eq!(data.cpu_history.len(), data.cores);
        assert_eq!(data.cpu_total.len(), HISTORY);
        assert!(data.processes.iter().all(|p| p.user == "demo" && p.command.starts_with("/opt/demo/")));
        assert!(data.processes.iter().filter(|p| p.parent != 0).all(|p| data.processes.iter().any(|parent| parent.pid == p.parent)));
        assert_eq!(data.networks[0].mac, "02:00:00:00:00:01");
        assert!(startups().iter().all(|s| s.file.is_empty()));
        assert!(services().iter().all(|s| s.name.starts_with("demo-")));
    }
}
