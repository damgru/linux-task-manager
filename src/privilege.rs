use crate::model::{efficiency, service_action, signal_process, Service};
use std::process::Command;
#[derive(Clone, Debug)]
pub enum ProcessAction {
    Signal(i32),
    Efficiency,
}
#[derive(Clone)]
pub enum PrivilegedAction {
    Process { pid: u32, start: u64, name: String, action: ProcessAction },
    Service { service: Service, action: String },
}
impl PrivilegedAction {
    pub fn run(&self) -> Result<(), String> {
        match self {
            Self::Process { pid, start, action, .. } => match action {
                ProcessAction::Signal(signal) => signal_process(*pid, *start, *signal),
                ProcessAction::Efficiency => efficiency(*pid, *start),
            },
            Self::Service { service, action } => service_action(service, action),
        }
    }
    pub fn description(&self) -> String {
        match self {
            Self::Process { pid, name, .. } => format!("{name} (PID {pid})"),
            Self::Service { service, action } => {
                format!("{} · {}", service.name, crate::i18n::tr(action))
            }
        }
    }
    fn arguments(&self) -> Vec<String> {
        match self {
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
    pub fn authenticate(&self) -> Result<(), String> {
        if matches!(self,Self::Service {service,..} if service.scope=="User") {
            return Err("User service authorization must be granted in the current user's systemd session.".into());
        }
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let output = Command::new("/usr/bin/pkexec")
            .arg("--disable-internal-agent")
            .arg(executable)
            .arg("--privileged-action")
            .args(self.arguments())
            .env("LC_ALL", "C")
            .output()
            .map_err(|_| "System authentication is unavailable. Install polkit and enable a desktop authentication agent.".to_string())?;
        match output.status.code() {
            Some(0) => Ok(()),
            Some(126) => Err("Authentication cancelled. No changes were made.".into()),
            Some(127) => Err("Authentication was denied or no desktop authentication agent is running.".into()),
            _ => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
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
pub fn helper(args: &[String]) -> Result<(), String> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("Administrator authentication is required.".into());
    }
    match args.first().map(String::as_str) {
        Some("signal" | "nice") if args.len() == 4 => {
            let pid = args[1].parse::<u32>().map_err(|_| "Invalid PID")?;
            let start = args[2].parse::<u64>().map_err(|_| "Invalid process identity")?;
            let value = args[3].parse::<i32>().map_err(|_| "Invalid action")?;
            if args[0] == "nice" && value == 10 {
                efficiency(pid, start)
            } else if args[0] == "signal" && [libc::SIGTERM, libc::SIGKILL, libc::SIGSTOP, libc::SIGCONT].contains(&value) {
                signal_process(pid, start, value)
            } else {
                Err("Unsupported process action".into())
            }
        }
        Some("service") if args.len() == 4 && args[3] == "System" && valid_service(&args[1], &args[2]) => {
            service_action(&Service { name: args[2].clone(), scope: "System".into(), ..Default::default() }, &args[1])
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
