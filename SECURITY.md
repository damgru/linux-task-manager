# Security policy

Only the latest published release receives security fixes; fixes land on main first.

Please report vulnerabilities privately using [GitHub private vulnerability reporting](https://github.com/damgru/linux-task-manager/security/advisories/new), rather than opening a public issue. Include the version, distribution/kernel, reproduction steps and expected impact, with private process names and credentials removed. If GitHub reporting is unavailable, contact the maintainer through the contact details on [their profile](https://github.com/damgru); do not publish exploit details in an issue.

Relevant boundaries include PID reuse, signal targeting, command launch, Polkit helper validation, service operations and parsing untrusted system/desktop metadata. The GUI runs as the current user; do not launch it as root. Authenticate only through the system's Polkit dialog for a specific operation.

Release files include SHA256SUMS.txt for download integrity. Checksums are not a signature; version 0.0.1 packages are unsigned.
