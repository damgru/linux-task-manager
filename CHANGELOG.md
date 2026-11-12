# Changelog

This project uses semantic versions and records user-visible changes here. Releases are prepared manually on Linux.

## 0.0.1 — 2026-11-12

Initial public release.

- Native Rust/egui Linux task manager with English and Polish interfaces.
- Separate Apps and Processes trees, configurable columns, search, sorting and details.
- CPU, memory, disk, network and available AMD/NVIDIA GPU telemetry.
- Process termination, suspend/resume, reduced priority, systemd services and XDG startup controls; narrow Polkit authorization when required.
- Persistent preferences, Ubuntu launcher integration and always-on-top through X11/XWayland.
- Reduced-refresh low-memory mode and bounded history.
- Safe `--demo` fixtures for public screenshots; system operations and persistence disabled.
- Linux x86_64 tar.gz, Debian package and AppImage; checksums and manual release instructions. No macOS/Windows builds or GitHub Actions pipelines.

Known limits: graphics-driver memory overhead remains significant, actual exhausted-memory startup is unverified, some GPU/process counters are unavailable, and always-on-top needs X11/XWayland.
