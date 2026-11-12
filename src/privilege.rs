use crate::model::{efficiency, service_action, signal_process, Service};
#[derive(Clone, Debug)]
pub enum ProcessAction {
    Signal(i32),
    Efficiency,
}
#[derive(Clone)]
pub enum PrivilegedAction {
    Process { pid: u32, start: u64, name: String, action: ProcessAction },
    Service { service: Service, action: String },
    Network { pid: u32, start: u64, name: String },
    Update { target: crate::updates::Target, key: (u32, u64) },
}
impl PrivilegedAction {
    pub fn run(&self) -> Result<(), String> {
        match self {
            Self::Process { pid, start, action, .. } => match action {
                ProcessAction::Signal(signal) => signal_process(*pid, *start, *signal),
                ProcessAction::Efficiency => efficiency(*pid, *start),
            },
            Self::Service { service, action } => service_action(service, action),
            Self::Update { target, .. } if target.format == "Flatpak" && target.scope == "--user" => crate::updates::install(target),
            _ => Err("Administrator authentication is required.".into()),
        }
    }
    pub fn description(&self) -> String {
        match self {
            Self::Process { pid, name, .. } => format!("{name} (PID {pid})"),
            Self::Network { pid, name, .. } => format!("{} · {name} (PID {pid})", crate::i18n::tr("Measure network usage")),
            Self::Update { target, .. } => format!(
                "{} · {} ({})",
                crate::i18n::tr(if target.format == "pacman" { "Update all system packages" } else { "Update" }),
                target.package,
                target.format
            ),
            Self::Service { service, action } => {
                format!("{} · {}", service.name, crate::i18n::tr(action))
            }
        }
    }
    fn arguments(&self) -> Vec<String> {
        match self {
            Self::Network { pid, start, .. } => vec!["network".into(), pid.to_string(), start.to_string()],
            Self::Update { target, .. } => target.arguments(),
            Self::Process { pid, start, action, .. } => vec![
                match action {
                    ProcessAction::Signal(_) => "signal",
                    ProcessAction::Efficiency => "nice",
                }
                .into(),
                pid.to_string(),
                start.to_string(),
                match action {
                    ProcessAction::Signal(s) => s.to_string(),
                    _ => "10".into(),
                },
            ],
            Self::Service { service, action } => {
                vec!["service".into(), action.clone(), service.name.clone(), service.scope.clone()]
            }
        }
    }
    pub fn authenticate(&self) -> Result<String, String> {
        if matches!(self, Self::Process { action: ProcessAction::Efficiency, .. }) {
            return Err("This process priority change cannot be safely run by the administrator helper.".into());
        }
        if matches!(self,Self::Service {service,..} if service.scope=="User") {
            return Err("User service authorization must be granted in the current user's systemd session.".into());
        }
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let executable = executable.to_str().ok_or("Executable path is unavailable")?;
        let arguments = self.arguments();
        let mut args = vec!["--disable-internal-agent", executable, "--privileged-action"];
        args.extend(arguments.iter().map(String::as_str));
        let output = crate::commands::run("/usr/bin/pkexec", &args, None)
            .map_err(|_| "System authentication is unavailable. Install polkit and enable a desktop authentication agent.".to_string())?;
        match output.code {
            0 => Ok(output.text),
            126 => Err("Authentication cancelled. No changes were made.".into()),
            127 => Err("Authentication was denied or no desktop authentication agent is running.".into()),
            _ => crate::commands::success(output),
        }
    }
}
pub fn permission_denied(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "(os error 1)",
        "(os error 13)",
        "access denied",
        "permission denied",
        "authentication is required",
        "interactive authentication required",
        "not authorized",
        "not permitted",
    ]
    .iter()
    .any(|s| message.contains(s))
}
/// A root-only, non-GUI entry point. Only these fixed operations are accepted;
/// arguments are never evaluated by a shell, and signals retain pidfd validation.
pub fn helper(args: &[String]) -> Result<String, String> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("Administrator authentication is required.".into());
    }
    match args.first().map(String::as_str) {
        Some("network") if args.len() == 3 => {
            let pid = args[1].parse::<u32>().map_err(|_| "Invalid PID")?;
            let start = args[2].parse::<u64>().map_err(|_| "Invalid process identity")?;
            crate::process_network::measure(pid, start).map(|rate| rate.to_string())
        }
        Some("update") if args.len() == 4 => {
            let target = crate::updates::Target { format: args[1].clone(), package: args[2].clone(), scope: args[3].clone() };
            // User Flatpaks must never be changed from the root helper.
            if target.format == "Flatpak" && target.scope != "--system" {
                return Err("Unsupported package update".into());
            }
            crate::updates::install(&target).map(|_| String::new())
        }
        Some("signal") if args.len() == 4 => {
            let pid = args[1].parse::<u32>().map_err(|_| "Invalid PID")?;
            let start = args[2].parse::<u64>().map_err(|_| "Invalid process identity")?;
            let value = args[3].parse::<i32>().map_err(|_| "Invalid action")?;
            if [libc::SIGTERM, libc::SIGKILL, libc::SIGSTOP, libc::SIGCONT].contains(&value) {
                signal_process(pid, start, value).map(|_| String::new())
            } else {
                Err("Unsupported process action".into())
            }
        }
        Some("service") if args.len() == 4 && args[3] == "System" && valid_service(&args[1], &args[2]) => {
            service_action(&Service { name: args[2].clone(), scope: "System".into(), ..Default::default() }, &args[1]).map(|_| String::new())
        }
        _ => Err("Unsupported privileged action".into()),
    }
}
fn valid_service(action: &str, unit: &str) -> bool {
    ["start", "stop", "restart"].contains(&action)
        && unit.ends_with(".service")
        && !unit.starts_with('-')
        && !unit.is_empty()
        && unit.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.@:-\\".contains(&b))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn efficiency_action_never_uses_the_root_helper() {
        let action = PrivilegedAction::Process { pid: 123, start: 456, name: "example".into(), action: ProcessAction::Efficiency };
        assert!(action.authenticate().unwrap_err().contains("cannot be safely run"));
    }
    #[test]
    fn only_allowlisted_service_actions() {
        assert!(valid_service("restart", "ssh.service"));
        assert!(!valid_service("enable", "ssh.service"));
        assert!(!valid_service("start", "--help.service"));
        assert!(!valid_service("start", "../../tmp/test.service"));
        assert!(!valid_service("start", "ssh.service;id"));
    }
    #[test]
    fn distinguishes_permissions_from_missing_processes() {
        assert!(permission_denied("Operation not permitted (os error 1)"));
        assert!(permission_denied("Interactive authentication required."));
        assert!(!permission_denied("The process exited or changed. Select it again."));
    }
}
