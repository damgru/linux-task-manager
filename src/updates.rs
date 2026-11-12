//! Checks run as the user. System updates use the narrow Polkit helper.
use crate::{app_details::Metadata, commands};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub format: String,
    pub package: String,
    pub scope: String,
}
#[derive(Clone, Debug)]
pub struct Available {
    pub version: String,
    pub target: Target,
}
pub fn safe_package(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.starts_with('-') && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"._+:-".contains(&c))
}
impl Target {
    pub fn valid(&self) -> bool {
        match self.format.as_str() {
            "deb" | "rpm" | "Snap" | "pacman" => safe_package(&self.package) && self.scope.is_empty(),
            "Flatpak" => {
                let parts: Vec<_> = self.package.split('/').collect();
                ["--user", "--system"].contains(&self.scope.as_str())
                    && parts.len() == 4
                    && parts[0] == "app"
                    && parts[1..].iter().all(|part| safe_package(part))
            }
            _ => false,
        }
    }
    pub fn arguments(&self) -> Vec<String> {
        vec!["update".into(), self.format.clone(), self.package.clone(), self.scope.clone()]
    }
}
fn candidate(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find_map(|s| s.strip_prefix("Candidate: ")).filter(|s| !s.is_empty() && *s != "(none)")
}
fn listed_version(text: &str, package: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next()? == package).then(|| fields.next().map(str::to_owned)).flatten()
    })
}
pub fn check(metadata: &Metadata) -> Result<Option<Available>, String> {
    if crate::demo::enabled() {
        return Err("Demo mode: system actions are disabled.".into());
    }
    let target = Target {
        format: metadata.format.clone(),
        package: if metadata.format == "Flatpak" { metadata.reference.clone() } else { metadata.package.clone() },
        scope: metadata.scope.clone(),
    };
    if !target.valid() {
        return Err("Update checks are unavailable for this installation. Use its software manager.".into());
    }
    let p = target.package.as_str();
    let version = match target.format.as_str() {
        "deb" => {
            let policy = commands::success(commands::query("/usr/bin/apt-cache", &["policy", "--", p])?)?;
            let version = candidate(&policy).ok_or("No package candidate is available. Refresh sources in your software manager.")?;
            let comparison = commands::query("/usr/bin/dpkg", &["--compare-versions", version, "gt", &metadata.version])?;
            match comparison.code {
                0 => Some(version.into()),
                1 => None,
                _ => return Err("Unable to compare package versions.".into()),
            }
        }
        "Snap" => {
            let list = commands::success(commands::query("/usr/bin/snap", &["refresh", "--list"])?)?;
            listed_version(&list, p)
        }
        "Flatpak" => {
            let list = commands::success(commands::query(
                "/usr/bin/flatpak",
                &[&target.scope, "remote-ls", "--updates", "--app", "--columns=ref,version"],
            )?)?;
            list.lines().find_map(|line| {
                let mut fields = line.split_whitespace();
                (fields.next()? == p).then(|| fields.next().filter(|v| *v != "-").unwrap_or("Update available").to_owned())
            })
        }
        "rpm" => {
            let list = commands::query("/usr/bin/dnf", &["-q", "check-update", p])?;
            if list.code != 0 && list.code != 100 {
                return Err(commands::success(list).unwrap_err());
            }
            list.text.lines().find_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields.len() >= 2 && (fields[0] == p || fields[0].rsplit_once('.').is_some_and(|(name, _)| name == p))).then(|| fields[1].into())
            })
        }
        "pacman" => {
            let list = commands::query("/usr/bin/pacman", &["-Qu", "--", p])?;
            if list.code == 1 && list.error.trim().is_empty() {
                None
            } else {
                let text = commands::success(list)?;
                text.lines().find_map(|line| line.split_once(" -> ").map(|(_, v)| v.trim().to_owned()))
            }
        }
        _ => unreachable!(),
    };
    Ok(version.map(|version| Available { version, target }))
}
pub fn install(target: &Target) -> Result<(), String> {
    if crate::demo::enabled() || !target.valid() {
        return Err("Unsupported package update".into());
    }
    let p = target.package.as_str();
    let (program, args) = match target.format.as_str() {
        "deb" => ("/usr/bin/apt-get", vec!["install", "--only-upgrade", "--yes", "--", p]),
        "rpm" => ("/usr/bin/dnf", vec!["upgrade", "--assumeyes", "--", p]),
        "Snap" => ("/usr/bin/snap", vec!["refresh", p]),
        // Arch does not support partial upgrades. Explain the full upgrade in the UI.
        "pacman" => ("/usr/bin/pacman", vec!["-Syu", "--noconfirm"]),
        "Flatpak" => ("/usr/bin/flatpak", vec![&target.scope, "update", "--noninteractive", p]),
        _ => return Err("Unsupported package update".into()),
    };
    // Do not kill a package transaction halfway through on a timer.
    commands::success(commands::run(program, &args, None)?).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_updates_and_parses_candidates() {
        assert_eq!(candidate("Installed: 1\n  Candidate: 2\n"), Some("2"));
        assert!(candidate("Candidate: (none)").is_none());
        assert_eq!(listed_version("Name Version\napp 2 rev\nother 9", "app"), Some("2".into()));
        assert!(listed_version("application 2", "app").is_none());
        for package in ["--help", "../app", "app;id", "app\nother", ""] {
            assert!(!safe_package(package));
        }
        assert!(Target { format: "deb".into(), package: "app:amd64".into(), scope: String::new() }.valid());
        assert!(Target { format: "Flatpak".into(), package: "app/org.test.App/x86_64/stable".into(), scope: "--user".into() }.valid());
        assert!(!Target { format: "Flatpak".into(), package: "app/org.test.App/x86_64/--help".into(), scope: "--system".into() }.valid());
        assert!(!Target { format: "Flatpak".into(), package: "org.test.App".into(), scope: "--installation=bad".into() }.valid());
    }
}
