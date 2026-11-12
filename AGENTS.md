# Linux Task Manager — working rules

Read this file, CONTRIBUTING.md and the relevant docs before changing code.

- Linux is the only supported platform. Do not add Windows/macOS artifacts or GitHub Actions workflows.
- Preserve the Windows-inspired workflow while presenting truthful Linux data. Unsupported counters show an explanation, never fabricated live values.
- Keep the collector independent of UI rendering; retain bounded samples, virtualized rows and a single latest-value slot. Avoid work on every repaint.
- Never signal a PID without checking its start time and using pidfd. Keep the GUI unprivileged; privileged actions must use the existing narrow helper allowlist.
- Treat procfs, sysfs, desktop entries and commands as untrusted input. Bound reads and handle missing data. Document each necessary unsafe syscall boundary.
- Keep changes focused. Add regression tests for behavior and security fixes. Run `scripts/check.sh` before pushing; no hosted pipeline substitutes for this local gate.
- Update CHANGELOG.md for user-visible changes; keep Cargo.toml, Cargo.lock, CLI version and release tag aligned. Follow docs/releasing.md for manual releases.
- All media shown publicly must use `--demo --language en`. Demo mode must never start a live collector, enumerate real services/startups, save settings, register launchers or execute system actions.
- Save screenshots, GIFs, generated packages and review materials outside the repository. Attach them to a release or PR; never commit them. Keep the original SVG source icon and record its origin in ATTRIBUTION.md.
- Publish only reviewed source and locked dependencies. Never commit target/, caches, secrets or local investigation files. After pushing, verify origin contains the complete work before removing any local leftovers; preserve unrelated or local-only changes and report git status.
