//! On-demand, read-only package inspection, kept off the rendering thread.
//! Never run an application's executable or interpolate metadata into a shell.
use crate::model::{parse_stat, Process};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub executable: String,
    pub format: String,
    pub package: String,
    pub version: String,
    pub note: String,
    pub scope: String,
    pub reference: String,
}

pub fn descendants(processes: &[Process], root: (u32, u64)) -> Vec<(usize, usize)> {
    let Some(index) = processes.iter().position(|p| (p.pid, p.start) == root) else { return vec![] };
    let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, p) in processes.iter().enumerate() {
        children.entry(p.parent).or_default().push(i);
    }
    let mut seen = HashSet::new();
    let mut stack = vec![(index, 0)];
    let mut result = Vec::new();
    while let Some((i, depth)) = stack.pop() {
        if !seen.insert(i) {
            continue;
        }
        result.push((i, depth));
        if let Some(children) = children.get(&processes[i].pid) {
            stack.extend(children.iter().rev().filter(|&&c| processes[c].start >= processes[i].start).map(|&c| (c, depth + 1)));
        }
    }
    result
}

fn read(path: impl AsRef<Path>) -> String {
    let mut text = String::new();
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(65536).read_to_string(&mut text);
    }
    text
}
fn current(p: &Process) -> bool {
    parse_stat(&read(format!("/proc/{}/stat", p.pid))).is_some_and(|(_, fields)| fields[19].parse::<u64>().ok() == Some(p.start))
}
fn safe_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.starts_with('-') && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"._+:-".contains(&c))
}
fn query(program: &str, args: &[&str]) -> Option<String> {
    // Fixed system tools only. Bound both output and runtime; always reap the
    // child. The reader drains concurrently so a full pipe cannot block exit.
    let mut child =
        Command::new(program).args(args).env("LC_ALL", "C").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.take(65536).read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let text = reader.join().ok()?;
    success.then_some(text)
}
fn field<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut active = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            active = line == format!("[{section}]");
        } else if active {
            if let Some((k, v)) = line.split_once('=') {
                if k == key {
                    return Some(v);
                }
            }
        }
    }
    None
}
fn env_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.split('\0').filter_map(|v| v.split_once('=')).find_map(|(k, v)| (k == key).then_some(v))
}
fn flatpak_version(text: &str, id: &str, branch: &str, arch: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        (fields.len() == 4 && fields[0] == id && fields[2] == branch && fields[3] == arch && !fields[1].is_empty()).then(|| fields[1].to_owned())
    })
}
fn deb_owner(text: &str, executable: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.split_once(": "))
        .find_map(|(package, path)| (path == executable && safe_name(package)).then(|| package.to_owned()))
}
fn package_pair(text: &str) -> Option<(String, String)> {
    let (name, version) = text.trim().split_once('\t')?;
    (safe_name(name) && !version.is_empty() && version.len() < 512 && !version.contains('\n')).then(|| (name.into(), version.into()))
}

pub fn inspect(p: &Process) -> Metadata {
    if crate::demo::enabled() {
        return demo(p);
    }
    if !current(p) {
        return Metadata { note: "The process exited or changed. Select it again.".into(), ..Default::default() };
    }
    let mut result = inspect_current(p);
    if !current(p) {
        result = Metadata { note: "The process exited or changed. Select it again.".into(), ..Default::default() };
    }
    result
}
fn inspect_current(p: &Process) -> Metadata {
    let base = format!("/proc/{}", p.pid);
    let mut result = Metadata {
        executable: fs::read_link(format!("{base}/exe")).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        ..Default::default()
    };
    let flatpak = read(format!("{base}/root/.flatpak-info"));
    if let Some(id) = field(&flatpak, "Application", "name").filter(|id| safe_name(id)) {
        result.format = "Flatpak".into();
        result.package = id.into();
        let app_path = field(&flatpak, "Instance", "app-path").unwrap_or_default();
        result.scope = if app_path.starts_with("/var/lib/flatpak/app/") {
            "--system"
        } else if app_path.contains("/.local/share/flatpak/app/") {
            "--user"
        } else {
            ""
        }
        .into();
        let branch = field(&flatpak, "Instance", "branch").unwrap_or("stable");
        let arch = field(&flatpak, "Instance", "arch").unwrap_or_default();
        if safe_name(branch) && safe_name(arch) {
            result.reference = format!("app/{id}/{arch}/{branch}");
        }
        if let Some(list) = query("/usr/bin/flatpak", &["list", "--app", "--columns=application,version,branch,arch"]) {
            result.version = flatpak_version(&list, id, branch, arch).unwrap_or_default();
        }
        return result;
    }
    let environment = read(format!("{base}/environ"));
    if let Some(name) = env_value(&environment, "SNAP_NAME").filter(|v| safe_name(v)) {
        // A launch environment alone may be inherited by unrelated children.
        // Confirm this executable belongs to the advertised Snap mount.
        if result.executable.starts_with(&format!("/snap/{name}/")) || result.executable.starts_with(&format!("/var/lib/snapd/snap/{name}/")) {
            result.format = "Snap".into();
            result.package = name.into();
            result.version = env_value(&environment, "SNAP_VERSION").unwrap_or_default().chars().take(256).collect();
            return result;
        }
    }
    if let Some(path) = env_value(&environment, "APPIMAGE").filter(|_| result.executable.starts_with("/tmp/.mount_")) {
        result.format = "AppImage".into();
        result.package = Path::new(path).file_name().unwrap_or_default().to_string_lossy().into_owned();
        return result;
    }
    if !result.executable.starts_with('/') || result.executable.ends_with(" (deleted)") {
        result.note = "Executable unavailable; package information could not be verified.".into();
        return result;
    }
    if let Some(owner) = query("/usr/bin/dpkg-query", &["-S", "--", &result.executable]).and_then(|text| deb_owner(&text, &result.executable)) {
        result.format = "deb".into();
        result.package = owner.clone();
        if let Some((_, version)) =
            query("/usr/bin/dpkg-query", &["-W", "-f=${binary:Package}\t${Version}\n", "--", &owner]).and_then(|t| package_pair(&t))
        {
            result.version = version;
        }
        return result;
    }
    if let Some((name, version)) =
        query("/usr/bin/rpm", &["-qf", "--qf", "%{NAME}\t%{VERSION}-%{RELEASE}\n", "--", &result.executable]).and_then(|t| package_pair(&t))
    {
        result.format = "rpm".into();
        result.package = name;
        result.version = version;
        return result;
    }
    if let Some(owner) = query("/usr/bin/pacman", &["-Qoq", "--", &result.executable]).map(|t| t.trim().to_owned()).filter(|v| safe_name(v)) {
        result.format = "pacman".into();
        result.package = owner.clone();
        if let Some(text) = query("/usr/bin/pacman", &["-Q", "--", &owner]) {
            result.version = text.trim().split_once(' ').map(|(_, v)| v.into()).unwrap_or_default();
        }
        return result;
    }
    result.note = "No supported package record found; this may be a manually installed application.".into();
    result
}

pub fn demo(p: &Process) -> Metadata {
    Metadata {
        executable: format!("/opt/demo/app-{}/bin/application", p.pid),
        format: "Flatpak".into(),
        package: "org.example.Orbit".into(),
        version: "12.4.1".into(),
        note: String::new(),
        scope: "--user".into(),
        reference: "app/org.example.Orbit/x86_64/stable".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subtree_keeps_hidden_children_but_rejects_reused_parents_and_cycles() {
        let p = |pid, parent, start| Process { pid, parent, start, ..Default::default() };
        let processes = vec![p(10, 12, 1), p(11, 10, 2), p(12, 11, 3), p(13, 10, 0), p(14, 0, 5)];
        assert_eq!(descendants(&processes, (10, 1)), vec![(0, 0), (1, 1), (2, 2)]);
        assert!(descendants(&processes, (10, 9)).is_empty());
    }
    #[test]
    fn inspects_only_the_expected_live_child_and_uses_real_package_records() {
        let executable = if Path::new("/usr/bin/sleep").exists() { "/usr/bin/sleep" } else { "/bin/sleep" };
        let mut child = Command::new(executable).arg("30").spawn().unwrap();
        let raw = read(format!("/proc/{}/stat", child.id()));
        let start = parse_stat(&raw).unwrap().1[19].parse().unwrap();
        let process = Process { pid: child.id(), start, ..Default::default() };
        let metadata = inspect(&process);
        let wrong = inspect(&Process { start: start + 1, ..process });
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(metadata.executable, fs::canonicalize(executable).unwrap().to_string_lossy());
        assert!(wrong.executable.is_empty());
        assert!(!wrong.note.is_empty());
        if let Some(owner) = query("/usr/bin/dpkg-query", &["-S", "--", &metadata.executable]).and_then(|t| deb_owner(&t, &metadata.executable)) {
            assert_eq!(metadata.format, "deb");
            assert_eq!(metadata.package, owner);
            assert!(!metadata.version.is_empty(), "installed Debian package version is read from its record");
        }
    }
    #[test]
    fn package_parsing_is_exact_and_does_not_guess_versions() {
        assert_eq!(deb_owner("other: /usr/bin/other\napp:amd64: /usr/bin/app\n", "/usr/bin/app"), Some("app:amd64".into()));
        assert!(deb_owner("other: /usr/bin/application", "/usr/bin/app").is_none());
        assert_eq!(package_pair("app:amd64\t2.4-1\n"), Some(("app:amd64".into(), "2.4-1".into())));
        assert!(package_pair("-option\t1").is_none());
        assert!(!safe_name("app; echo bad"));
        assert_eq!(flatpak_version("org.test.App\t1.2\tstable\tx86_64", "org.test.App", "stable", "x86_64"), Some("1.2".into()));
        assert!(flatpak_version("org.test.App\t1.2\tbeta\tx86_64", "org.test.App", "stable", "x86_64").is_none());
        assert_eq!(field("[Application]\nname=org.test.App\n[Instance]\narch=x86_64", "Instance", "arch"), Some("x86_64"));
        assert_eq!(env_value("SECRET=hidden\0SNAP_NAME=app\0", "SNAP_NAME"), Some("app"));
    }
}
