//! Docker CLI adapter. No shell, no root escalation, bounded output/time and immutable IDs.
use crate::{commands, demo};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, time::Duration};

const CONTAINER_FORMAT: &str = r#"{"id":{{json .Id}},"name":{{json .Name}},"image":{{json .Image}},"image_name":{{json .Config.Image}},"state":{"Status":{{json .State.Status}},"StartedAt":{{json .State.StartedAt}},"FinishedAt":{{json .State.FinishedAt}},"ExitCode":{{json .State.ExitCode}},"Health":{"Status":{{with (index .State "Health")}}{{json .Status}}{{else}}null{{end}}}},"created":{{json .Created}},"restart":{{json .HostConfig.RestartPolicy.Name}},"project":{{json (index .Config.Labels "com.docker.compose.project")}},"service":{{json (index .Config.Labels "com.docker.compose.service")}},"workdir":{{json (index .Config.Labels "com.docker.compose.project.working_dir")}},"ports":{{json .NetworkSettings.Ports}},"networks":{{json .NetworkSettings.Networks}},"mounts":{{json .Mounts}},"size":{{json .SizeRw}},"root_size":{{json .SizeRootFs}}}"#;
const IMAGE_FORMAT: &str = r#"{"id":{{json .Id}},"tags":{{json .RepoTags}},"digests":{{json .RepoDigests}},"parent":{{json (index . "Parent")}},"size":{{json .Size}},"created":{{json .Created}},"os":{{json .Os}},"arch":{{json .Architecture}},"layers":{{json .RootFS.Layers}}}"#;

#[derive(Clone, Debug)]
pub struct Backend {
    program: String,
    // Pin the selected context/host so a later CLI context switch cannot redirect actions.
    endpoint: Vec<String>,
    pub context: String,
    endpoint_error: Option<String>,
}
impl Backend {
    pub fn detect() -> Option<Self> {
        if demo::enabled() {
            return None;
        }
        let program =
            ["/usr/bin/docker", "/usr/local/bin/docker", "/bin/docker", "/snap/bin/docker"].into_iter().find(|p| Path::new(p).is_file())?.to_owned();
        commands::success(commands::run(&program, &["--version"], Some(Duration::from_secs(3))).ok()?).ok()?;
        let explicit_context = std::env::var("DOCKER_CONTEXT").ok().filter(|s| !s.is_empty());
        let explicit_host = std::env::var("DOCKER_HOST").ok().filter(|s| !s.is_empty());
        let pinned = if let Some(context) = explicit_context {
            Ok((vec!["--context".into(), context.clone()], context))
        } else if let Some(host) = explicit_host {
            Ok((vec!["--host".into(), host.clone()], host))
        } else {
            commands::run(&program, &["context", "show"], Some(Duration::from_secs(3))).and_then(commands::success).and_then(|text| {
                let context = text.trim().to_owned();
                if context.is_empty() || context.len() > 1024 {
                    return Err("Unable to read Docker context".into());
                }
                Ok((vec!["--context".into(), context.clone()], context))
            })
        };
        let (endpoint, context, endpoint_error) = match pinned {
            Ok((endpoint, context)) => (endpoint, context, None),
            Err(error) => (vec![], String::new(), Some(error)),
        };
        Some(Self { program, endpoint, context, endpoint_error })
    }
    fn query(&self, args: &[&str]) -> Result<String, String> {
        if demo::enabled() {
            return Err("Demo mode: system actions are disabled.".into());
        }
        if let Some(error) = &self.endpoint_error {
            return Err(error.clone());
        }
        let args: Vec<_> = self.endpoint.iter().map(String::as_str).chain(args.iter().copied()).collect();
        commands::success(commands::query(&self.program, &args)?)
    }
    pub fn collect(&self, target: Option<&Target>) -> Result<Snapshot, String> {
        let container_ids = ids(&self.query(&["container", "ls", "--all", "--quiet", "--no-trunc"])?)?;
        let image_ids = ids(&self.query(&["image", "ls", "--all", "--quiet", "--no-trunc"])?)?;
        let mut containers = vec![];
        for batch in container_ids.chunks(4) {
            let mut args = vec!["container", "inspect", "--size", "--format", CONTAINER_FORMAT];
            args.extend(batch.iter().map(String::as_str));
            containers.extend(json_lines(&self.query(&args)?)?.iter().map(Container::parse).collect::<Result<Vec<_>, _>>()?);
        }
        let mut images = vec![];
        for batch in image_ids.chunks(4) {
            let mut args = vec!["image", "inspect", "--format", IMAGE_FORMAT];
            args.extend(batch.iter().map(String::as_str));
            images.extend(json_lines(&self.query(&args)?)?.iter().map(Image::parse).collect::<Result<Vec<_>, _>>()?);
        }
        let mut warning = String::new();
        if containers.iter().any(Container::running) {
            // A failed stats request must not disguise valid inventory as an empty daemon.
            match self.query(&["stats", "--no-stream", "--no-trunc", "--format", "{{json .}}"]).and_then(|text| json_lines(&text)) {
                Ok(stats) => {
                    for stat in stats {
                        if let Some(c) = containers.iter_mut().find(|c| c.id == string(&stat, "ID")) {
                            if c.running() {
                                c.stats = Some(Stats::parse(&stat));
                            }
                        }
                    }
                }
                Err(e) => warning = e,
            }
        }
        let snapshot = Snapshot { containers, images, warning, ..Default::default() };
        let logs = target.filter(|t| !matches!(t, Target::Image(_))).map(|t| self.logs(&snapshot.members(t))).unwrap_or_default();
        Ok(Snapshot { logs, ..snapshot })
    }
    fn logs(&self, members: &[&Container]) -> String {
        let mut output = String::new();
        for c in members.iter().take(8) {
            output.push_str(&format!("── {} ──\n", c.name));
            // Docker logs writes the container's stderr to the CLI's stderr even on success.
            let args: Vec<_> = self.endpoint.iter().map(String::as_str).chain(["logs", "--timestamps", "--tail", "40", &c.id]).collect();
            let result = commands::run(&self.program, &args, Some(Duration::from_secs(5)));
            match result {
                Ok(result) if result.code == 0 => {
                    output.push_str(&result.text);
                    output.push_str(&result.error);
                }
                Ok(result) => output.push_str(&result.error),
                Err(e) => output.push_str(&e),
            }
            output.push('\n');
            if output.len() > 48_000 {
                break;
            }
        }
        // Control sequences are never interpreted; keep only printable text/newlines.
        output.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').take(48_000).collect()
    }
    pub fn action(&self, action: Action, ids: &[String], force: bool) -> Result<(), String> {
        self.query(&action_args(action, ids, force, demo::enabled())?).map(|_| ())
    }
}

pub fn valid_id(id: &str) -> bool {
    let hex = id.strip_prefix("sha256:").unwrap_or(id);
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}
fn ids(text: &str) -> Result<Vec<String>, String> {
    let mut ids = vec![];
    for id in text.lines().filter(|id| !id.is_empty()) {
        if !valid_id(id) {
            return Err("Invalid Docker identifier".into());
        }
        if !ids.iter().any(|v| v == id) {
            ids.push(id.to_owned());
        }
        if ids.len() > 256 {
            return Err("Docker inventory exceeds the safe limit of 256 items.".into());
        }
    }
    Ok(ids)
}
fn json_lines(text: &str) -> Result<Vec<Value>, String> {
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).map_err(|e| format!("Docker JSON: {e}"))).collect()
}
fn string(v: &Value, field: &str) -> String {
    v[field].as_str().unwrap_or_default().chars().filter(|c| !c.is_control()).take(2048).collect()
}
fn strings(v: &Value, field: &str) -> Vec<String> {
    v[field].as_array().map(|a| a.iter().filter_map(Value::as_str).take(256).map(str::to_owned).collect()).unwrap_or_default()
}
#[derive(Clone, Default, Debug)]
pub struct Stats {
    pub cpu: Option<f32>,
    pub memory: Option<u64>,
    pub limit: Option<u64>,
    pub read: Option<u64>,
    pub write: Option<u64>,
    pub received: Option<u64>,
    pub sent: Option<u64>,
    pub pids: Option<u64>,
}
impl Stats {
    fn parse(v: &Value) -> Self {
        let pair = |field| {
            let s = string(v, field);
            let (a, b) = s.split_once('/')?;
            Some((quantity(a), quantity(b)))
        };
        let (memory, limit) = pair("MemUsage").unwrap_or_default();
        let (read, write) = pair("BlockIO").unwrap_or_default();
        let (received, sent) = pair("NetIO").unwrap_or_default();
        let cpu = string(v, "CPUPerc").trim_end_matches('%').parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0);
        Self { cpu, memory, limit, read, write, received, sent, pids: string(v, "PIDs").parse().ok() }
    }
}
fn quantity(text: &str) -> Option<u64> {
    let text = text.trim();
    let at = text.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(text.len());
    let number = text[..at].parse::<f64>().ok()?;
    let factor = match text[at..].trim() {
        "B" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0_f64.powi(4),
        _ => return None,
    };
    let value = number * factor;
    (value.is_finite() && value >= 0.0 && value < u64::MAX as f64).then_some(value as u64)
}
#[derive(Clone, Default, Debug)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub image_name: String,
    pub status: String,
    pub health: String,
    pub project: String,
    pub service: String,
    pub workdir: String,
    pub created: String,
    pub started: String,
    pub finished: String,
    pub restart: String,
    pub exit_code: Option<i64>,
    pub size: Option<u64>,
    pub root_size: Option<u64>,
    pub ports: String,
    pub networks: String,
    pub mounts: String,
    pub stats: Option<Stats>,
}
impl Container {
    fn parse(v: &Value) -> Result<Self, String> {
        let id = string(v, "id");
        if !valid_id(&id) {
            return Err("Invalid Docker identifier".into());
        }
        let state = &v["state"];
        Ok(Self {
            id,
            name: string(v, "name").trim_start_matches('/').into(),
            image: string(v, "image"),
            image_name: string(v, "image_name"),
            status: string(state, "Status"),
            health: string(&state["Health"], "Status"),
            project: string(v, "project"),
            service: string(v, "service"),
            workdir: string(v, "workdir"),
            created: string(v, "created"),
            started: string(state, "StartedAt"),
            finished: string(state, "FinishedAt"),
            restart: string(v, "restart"),
            exit_code: state["ExitCode"].as_i64(),
            size: v["size"].as_u64(),
            root_size: v["root_size"].as_u64(),
            ports: serde_json::to_string_pretty(&v["ports"]).unwrap_or_default(),
            networks: v["networks"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(", ")).unwrap_or_default(),
            mounts: serde_json::to_string_pretty(&v["mounts"]).unwrap_or_default(),
            stats: None,
        })
    }
    pub fn running(&self) -> bool {
        self.status == "running" || self.status == "restarting"
    }
    pub fn active(&self) -> bool {
        self.running() || self.status == "paused"
    }
}
#[derive(Clone, Default, Debug)]
pub struct Image {
    pub id: String,
    pub tags: Vec<String>,
    pub digests: Vec<String>,
    pub parent: String,
    pub size: Option<u64>,
    pub created: String,
    pub platform: String,
    pub layers: Vec<String>,
}
impl Image {
    fn parse(v: &Value) -> Result<Self, String> {
        let id = string(v, "id");
        if !valid_id(&id) {
            return Err("Invalid Docker identifier".into());
        }
        Ok(Self {
            id,
            tags: strings(v, "tags"),
            digests: strings(v, "digests"),
            parent: string(v, "parent"),
            size: v["size"].as_u64(),
            created: string(v, "created"),
            platform: format!("{}/{}", string(v, "os"), string(v, "arch")),
            layers: strings(v, "layers"),
        })
    }
    pub fn name(&self) -> &str {
        self.tags.first().map(String::as_str).unwrap_or("<untagged>")
    }
    pub fn derived_from(&self, base: &Self) -> bool {
        self.id != base.id
            && (self.parent == base.id || (!base.layers.is_empty() && self.layers.len() > base.layers.len() && self.layers.starts_with(&base.layers)))
    }
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Target {
    Container(String),
    Image(String),
    Project(String),
}
#[derive(Clone, Default, Debug)]
pub struct Snapshot {
    pub containers: Vec<Container>,
    pub images: Vec<Image>,
    pub logs: String,
    pub warning: String,
}
impl Snapshot {
    pub fn projects(&self) -> BTreeMap<String, Vec<&Container>> {
        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for c in &self.containers {
            groups.entry(c.project.clone()).or_default().push(c);
        }
        groups
    }
    pub fn members(&self, target: &Target) -> Vec<&Container> {
        self.containers
            .iter()
            .filter(|c| match target {
                Target::Container(id) => c.id == *id,
                Target::Image(id) => c.image == *id,
                Target::Project(name) => c.project == *name,
            })
            .collect()
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Start,
    Stop,
    Kill,
    Restart,
    RemoveContainer,
    RemoveImage,
}
impl Action {
    pub fn command(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Kill => "kill",
            Self::Restart => "restart",
            Self::RemoveContainer | Self::RemoveImage => "rm",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Stop => "Stop",
            Self::Kill => "Force stop",
            Self::Restart => "Restart",
            Self::RemoveContainer | Self::RemoveImage => "Delete",
        }
    }
    pub fn destructive(self) -> bool {
        matches!(self, Self::RemoveContainer | Self::RemoveImage)
    }
    pub fn requires_confirmation(self, preference: bool) -> bool {
        preference || self == Self::Kill || self.destructive()
    }
}
fn action_args(action: Action, ids: &[String], force: bool, is_demo: bool) -> Result<Vec<&str>, String> {
    if is_demo {
        return Err("Demo mode: system actions are disabled.".into());
    }
    if ids.is_empty()
        || ids.len() > 256
        || !ids.iter().all(|id| valid_id(id) && (action == Action::RemoveImage || !id.starts_with("sha256:")))
        || (force && action != Action::RemoveContainer)
    {
        return Err("Invalid Docker identifier".into());
    }
    let mut args = vec![if action == Action::RemoveImage { "image" } else { "container" }, action.command()];
    if action == Action::Stop {
        args.extend(["--time", "10"]);
    }
    if force {
        args.push("--force");
    }
    args.push("--");
    args.extend(ids.iter().map(String::as_str));
    Ok(args)
}

pub fn fixture() -> Snapshot {
    let layers = vec![format!("sha256:{}", "a".repeat(64)), format!("sha256:{}", "b".repeat(64))];
    let images = [
        ("nginx:1.27-alpine", 52_428_800),
        ("postgres:17", 438_304_768),
        ("redis:7-alpine", 41_943_040),
        ("workspace-api:latest", 286_261_248),
        ("node:22-alpine", 159_383_552),
        ("<untagged>", 87_031_808),
    ]
    .iter()
    .enumerate()
    .map(|(i, (name, size))| Image {
        id: format!("sha256:{:064x}", i + 1),
        tags: vec![(*name).into()],
        size: Some(*size),
        created: "2026-11-10T09:30:00Z".into(),
        platform: "linux/amd64".into(),
        layers: if i == 3 {
            [layers.clone(), vec![format!("sha256:{}", "c".repeat(64))]].concat()
        } else if i == 4 {
            layers.clone()
        } else {
            vec![format!("sha256:{:064x}", i + 20)]
        },
        ..Default::default()
    })
    .collect::<Vec<_>>();
    let containers = [
        ("workspace-web-1", 0, "workspace", "web", true, 0.8, 24_117_248),
        ("workspace-api-1", 3, "workspace", "api", true, 4.2, 178_257_920),
        ("workspace-db-1", 1, "workspace", "db", true, 1.7, 234_881_024),
        ("workspace-cache-1", 2, "workspace", "cache", true, 0.2, 12_582_912),
        ("analytics-db-1", 1, "analytics", "db", true, 2.1, 150_994_944),
        ("analytics-worker-1", 4, "analytics", "worker", false, 0.0, 0),
        ("redis-local", 2, "", "", false, 0.0, 0),
    ]
    .iter()
    .enumerate()
    .map(|(i, (name, img, project, service, running, cpu, mem))| Container {
        id: format!("{:064x}", i + 101),
        name: (*name).into(),
        image: images[*img].id.clone(),
        image_name: images[*img].name().into(),
        project: (*project).into(),
        service: (*service).into(),
        workdir: format!("/home/demo/projects/{project}"),
        status: if *running { "running" } else { "exited" }.into(),
        health: if *running { "healthy" } else { "" }.into(),
        size: Some((i as u64 + 1) * 1_048_576),
        root_size: images[*img].size,
        created: "2026-11-10T09:30:00Z".into(),
        started: "2026-11-12T08:00:00Z".into(),
        restart: "unless-stopped".into(),
        ports: if i == 0 { "80/tcp -> 127.0.0.1:8080" } else { "Internal network" }.into(),
        networks: format!("{project}_default"),
        mounts: "/var/lib/data -> /data (volume, rw)".into(),
        stats: running.then_some(Stats {
            cpu: Some(*cpu),
            memory: Some(*mem),
            limit: Some(8_589_934_592),
            read: Some(12_582_912),
            write: Some(4_194_304),
            received: Some(2_097_152),
            sent: Some(1_048_576),
            pids: Some(8 + i as u64),
        }),
        ..Default::default()
    })
    .collect();
    Snapshot { containers, images, logs: "── workspace-api-1 ──\n2026-11-12T10:42:01Z  INFO  Server listening on :3000\n2026-11-12T10:42:02Z  INFO  Database connection established\n2026-11-12T10:43:12Z  GET   /api/health  200  2ms\n2026-11-12T10:43:18Z  GET   /api/projects  200  12ms\n".into(), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Read-only integration probe requires a running Docker daemon"]
    fn live_inventory_probe() {
        let backend = Backend::detect().expect("Docker CLI installed");
        let data = backend.collect(None).expect("Docker inventory readable");
        assert!(data.containers.iter().all(|c| valid_id(&c.id)));
        assert!(data.images.iter().all(|i| valid_id(&i.id)));
        if let Some(c) = data.containers.first() {
            let logs = backend.logs(&[c]);
            assert!(logs.chars().count() <= 48_000);
        }
        eprintln!(
            "Read-only Docker probe: {} containers, {} images, {} reporting stats",
            data.containers.len(),
            data.images.len(),
            data.containers.iter().filter(|c| c.stats.is_some()).count()
        );
    }
    #[test]
    fn quantities_and_missing_stats_are_truthful() {
        assert_eq!(quantity(" 2.5MiB "), Some(2_621_440));
        assert_eq!(quantity("13.3kB"), Some(13_300));
        for bad in ["N/A", "--", "", "NaNB", "-1B", "5XB"] {
            assert_eq!(quantity(bad), None);
        }
        let stats = Stats::parse(&serde_json::json!({"CPUPerc":"150.5%","MemUsage":"2MiB / 1GiB", "BlockIO":"4kB / 0B", "PIDs":"12"}));
        assert_eq!(stats.cpu, Some(150.5));
        assert_eq!(stats.memory, Some(2_097_152));
        assert_eq!(stats.received, None);
        assert_eq!(stats.write, Some(0));
        assert_eq!(Stats::parse(&Value::Null).cpu, None);
    }
    #[test]
    fn identities_and_demo_guard_prevent_command_injection() {
        let id = "a".repeat(64);
        assert!(action_args(Action::Stop, std::slice::from_ref(&id), false, false).is_ok());
        assert!(action_args(Action::Stop, &[id], false, true).is_err());
        for bad in ["--all", "hello;docker kill", "abc", "", &format!("sha256:{}", "a".repeat(64))] {
            assert!(action_args(Action::Stop, &[bad.into()], false, false).is_err());
        }
        assert!(ids("--all").is_err());
        assert!(json_lines("{broken}").is_err());
    }
    #[test]
    fn deletion_commands_are_id_only_and_force_is_explicit() {
        let containers = vec!["a".repeat(64), "b".repeat(64)];
        let image = vec![format!("sha256:{}", "c".repeat(64))];
        assert_eq!(
            action_args(Action::RemoveContainer, &containers, false, false).unwrap(),
            ["container", "rm", "--", &containers[0], &containers[1]]
        );
        assert_eq!(
            action_args(Action::RemoveContainer, &containers, true, false).unwrap(),
            ["container", "rm", "--force", "--", &containers[0], &containers[1]]
        );
        assert_eq!(action_args(Action::RemoveImage, &image, false, false).unwrap(), ["image", "rm", "--", &image[0]]);
        assert!(action_args(Action::RemoveImage, &image, true, false).is_err(), "never force image deletion");
        for action in [Action::RemoveContainer, Action::RemoveImage] {
            assert!(action.requires_confirmation(false), "deletion cannot bypass confirmation preferences");
            assert!(action_args(action, &containers, true, true).is_err(), "demo cannot delete");
            assert!(action_args(action, &[], false, false).is_err());
            assert!(action_args(action, &vec![containers[0].clone(); 257], false, false).is_err());
            for bad in ["--all", "nginx:latest", "hello;docker rm", "abc", ""] {
                assert!(action_args(action, &[bad.into()], false, false).is_err());
            }
        }
        assert!(action_args(Action::RemoveContainer, &image, false, false).is_err());
        assert!(action_args(Action::Stop, &containers, true, false).is_err());
    }
    #[test]
    fn grouping_usage_and_layer_dependencies_include_stopped_containers() {
        let data = fixture();
        assert_eq!(data.projects()["workspace"].len(), 4);
        assert_eq!(data.projects()["analytics"].len(), 2);
        assert_eq!(data.projects()[""].len(), 1);
        assert_eq!(data.members(&Target::Image(data.images[2].id.clone())).len(), 2);
        assert!(data.images[3].derived_from(&data.images[4]));
        assert!(!data.images[4].derived_from(&data.images[3]));
        assert!(!data.images[4].derived_from(&data.images[4]));
        let c = Container::parse(&serde_json::json!({"id":"a".repeat(64), "name":"/api", "state":{"Status":"paused"}, "project":"test"})).unwrap();
        assert!(c.active());
        assert!(!c.running());
        assert_eq!(c.size, None);
        assert_eq!(c.name, "api");
    }
}
