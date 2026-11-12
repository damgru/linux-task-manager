//! NVIDIA uses the same read-only nvidia-smi interface as Vitals. Keep one
//! process and a bounded latest-value slot, rather than spawning per sample.
use crate::model::{push, Gpu};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
#[derive(Clone, Debug)]
pub struct NvidiaSample {
    pub bus: String,
    pub name: String,
    pub busy: Option<f32>,
    pub used: Option<u64>,
    pub total: Option<u64>,
    pub temperature: Option<f32>,
    pub encoder: Option<f32>,
    pub decoder: Option<f32>,
    pub power: Option<f32>,
    pub driver: String,
}
fn value(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0)
}
pub fn bus_id(s: &str) -> String {
    let s = s.trim().to_ascii_lowercase();
    if s.len() > 12 {
        s[s.len() - 12..].to_string()
    } else {
        s
    }
}
pub fn parse_nvidia(line: &str) -> Option<NvidiaSample> {
    let f: Vec<_> = line.trim().split(',').map(str::trim).collect();
    if f.len() != 10 || !f[0].contains(':') {
        return None;
    }
    let memory = |s: &str| value(s).map(|v| (v as f64 * 1048576.0) as u64);
    Some(NvidiaSample {
        bus: bus_id(f[0]),
        name: f[1].into(),
        busy: value(f[2]),
        used: memory(f[3]),
        total: memory(f[4]),
        temperature: value(f[5]),
        encoder: value(f[6]),
        decoder: value(f[7]),
        power: value(f[8]),
        driver: f[9].into(),
    })
}
pub struct NvidiaReader {
    child: Option<Child>,
    latest: Arc<Mutex<HashMap<String, NvidiaSample>>>,
}
impl NvidiaReader {
    pub fn start() -> Self {
        let latest = Arc::new(Mutex::new(HashMap::new()));
        let child = Command::new("nvidia-smi")
            .args([
                "--query-gpu=pci.bus_id,name,utilization.gpu,memory.used,memory.total,temperature.gpu,utilization.encoder,utilization.decoder,power.draw,driver_version",
                "--format=csv,noheader,nounits",
                "-l",
                "2",
            ])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = child.ok();
        if let Some(stdout) = child.as_mut().and_then(|c| c.stdout.take()) {
            let slot = latest.clone();
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(sample) = parse_nvidia(&line) {
                        if let Ok(mut map) = slot.lock() {
                            if map.len() < 64 || map.contains_key(&sample.bus) {
                                map.insert(sample.bus.clone(), sample);
                            }
                        }
                    }
                }
            });
        }
        Self { child, latest }
    }
    pub fn merge(&mut self, gpus: &mut Vec<Gpu>) {
        let running = self.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if !running {
            for g in gpus.iter_mut().filter(|g| g.nvidia) {
                g.encoder = None;
                g.decoder = None;
                g.power = None;
                g.source = "sysfs (DRM / hwmon)".into();
                g.note = "NVIDIA telemetry is unavailable. Check that the NVIDIA driver and nvidia-smi work in this session.".into();
            }
            return;
        }
        let Ok(samples) = self.latest.lock() else {
            return;
        };
        for sample in samples.values() {
            let index = gpus.iter().position(|g| g.bus == sample.bus);
            let i = index.unwrap_or_else(|| {
                gpus.push(Gpu { bus: sample.bus.clone(), nvidia: true, ..Default::default() });
                gpus.len() - 1
            });
            let g = &mut gpus[i];
            g.name = sample.name.clone();
            g.busy = sample.busy;
            g.used = sample.used;
            g.total = sample.total;
            g.temperature = sample.temperature;
            g.encoder = sample.encoder;
            g.decoder = sample.decoder;
            g.power = sample.power;
            g.driver = sample.driver.clone();
            g.source = "nvidia-smi (NVIDIA)".into();
            g.note.clear();
            if let Some(n) = g.busy {
                push(&mut g.history, n);
            }
            if let Some(n) = g.encoder {
                push(&mut g.encode_history, n);
            }
            if let Some(n) = g.decoder {
                push(&mut g.decode_history, n);
            }
        }
    }
}
impl Drop for NvidiaReader {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_real_units_and_unavailable_values() {
        let g = parse_nvidia("00000000:01:00.0, NVIDIA GeForce RTX 3060 Ti, 12, 512, 8192, 47, 3, [N/A], 35.50, 580.95.05").unwrap();
        assert_eq!(g.bus, "0000:01:00.0");
        assert_eq!(g.used, Some(512 * 1048576));
        assert_eq!(g.busy, Some(12.0));
        assert_eq!(g.decoder, None);
        assert!(parse_nvidia("NVIDIA-SMI has failed").is_none());
        assert_eq!(value("NaN"), None);
    }
}
