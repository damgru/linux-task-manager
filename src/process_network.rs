//! Short, explicitly authorized per-process measurement. Namespace/interface
//! totals from /proc/<pid>/net/dev are never presented as process traffic.
use crate::{
    commands,
    model::{parse_stat, Process},
};
use std::{fs, io::Read, path::Path};
fn current(pid: u32, start: u64) -> bool {
    let mut text = String::new();
    fs::File::open(format!("/proc/{pid}/stat")).is_ok_and(|f| f.take(65536).read_to_string(&mut text).is_ok())
        && parse_stat(&text).is_some_and(|(_, fields)| fields[19].parse::<u64>().ok() == Some(start))
}
pub fn request(process: &Process) -> crate::privilege::PrivilegedAction {
    crate::privilege::PrivilegedAction::Network { pid: process.pid, start: process.start, name: process.name.clone() }
}
fn parse(text: &str, pid: u32) -> Option<f32> {
    let mut sample = None;
    for line in text.lines() {
        if line.starts_with("Refreshing:") {
            sample = None;
        }
        let mut fields = line.trim_end().rsplitn(3, '\t');
        let (Some(received), Some(sent), Some(identity)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let Some((path_pid, _uid)) = identity.rsplit_once('/') else { continue };
        let Some((_path, value)) = path_pid.rsplit_once('/') else { continue };
        if value.parse::<u32>().ok() != Some(pid) {
            continue;
        }
        let (Ok(sent), Ok(received)) = (sent.parse::<f32>(), received.parse::<f32>()) else { continue };
        let rate = (sent + received) * 1024.0;
        if sent >= 0.0 && received >= 0.0 && rate.is_finite() {
            sample = Some(sample.unwrap_or(0.0) + rate);
        }
    }
    sample.filter(|value| value.is_finite())
}
pub fn measure(pid: u32, start: u64) -> Result<f32, String> {
    if crate::demo::enabled() {
        return Err("Demo mode: system actions are disabled.".into());
    }
    if !current(pid, start) {
        return Err("The process exited or changed. Select it again.".into());
    }
    let program = ["/usr/sbin/nethogs", "/usr/bin/nethogs"]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .ok_or("Per-process network measurement requires NetHogs. Install nethogs with your software manager and try again.")?;
    let output = commands::query(program, &["-t", "-c", "3", "-d", "1", "-C", "-P", &pid.to_string()])?;
    if !current(pid, start) {
        return Err("The process exited or changed. Select it again.".into());
    }
    if output.code != 0 {
        return Err("Network capture failed. Check NetHogs support and network permissions.".into());
    }
    parse(&output.text, pid)
        .ok_or_else(|| "No traffic could be attributed to this process during the sample. Try again while it is using the network.".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_pid_latest_sample_and_missing_are_distinct_from_zero() {
        assert_eq!(
            parse("Refreshing:\n/usr/bin/app/12/1000\t1\t2\n/usr/bin/other/123/1000\t99\t99\nRefreshing:\n/usr/bin/app/12/1000\t0\t0", 12),
            Some(0.0)
        );
        assert_eq!(parse("/some/path/app/12/1000\t1.5\t2.5", 12), Some(4096.0));
        assert!(parse("Refreshing:\n/usr/bin/app/12/1000\t1\t2\nRefreshing:\n", 12).is_none());
        assert!(parse("/app/12/1000\tNaN\t1\n/app/12/1000\t-1\t2", 12).is_none());
        assert_eq!(parse("/path with spaces/app/12/1000\t1\t2", 12), Some(3072.0));
        assert!(measure(std::process::id(), u64::MAX).is_err(), "reject reuse before capture");
    }
}
