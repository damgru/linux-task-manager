# Contributing

Report bugs and propose features in [GitHub Issues](https://github.com/damgru/linux-task-manager/issues). For security problems, use [SECURITY.md](SECURITY.md).
Keep discussion respectful and practical; include reproducible steps, the app version, distribution, display session (X11/Wayland), expected result and actual result. Remove private process names, paths and command lines from reports.

## Develop on Linux

Install Rust 1.93.1 or newer, a C compiler, pkg-config and the X11/Wayland/OpenGL development libraries. On Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config libx11-dev libxi-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
cargo run --locked
```

The app uses eframe/egui with the Glow renderer. See [architecture](docs/architecture.md), [AGENTS.md](AGENTS.md) and the README for platform limitations.

## Runtime libraries

The .deb installs these automatically. For a minimal Linux installation using the portable archive or AppImage:

```sh
sudo apt install libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libwayland-cursor0 libwayland-egl1 libegl1 libgl1
```

Install your distribution's graphics driver as well; these are runtime libraries, not bundled GPU drivers.

## Send a change

1. Branch from current main. Keep a PR focused on one behavior.
2. Add a regression test where it provides meaningful coverage. Avoid new dependencies unless needed; update lockfile and third-party notices if dependencies change.
3. Run `scripts/check.sh` (format, Clippy with warnings denied, tests, release build and the live self-test). This repository deliberately has **no GitHub Actions pipeline**; include the local check result in the PR.
4. For visible behavior, capture the actual build using `--demo --language en`, save media outside the checkout, and attach it to the PR. Never publish real process/user/system data.
5. Explain the resulting behavior and any limits. Update CHANGELOG.md for user-visible changes.

The self-test reads procfs and terminates only its own disposable `sleep` child. Unit/UI tests run without a graphical session; visual checks require a Linux desktop. A successful test is not proof that startup will succeed in a real out-of-memory situation.

Contributions are licensed under the project's MIT license. Assets must be original or openly licensed and recorded in ATTRIBUTION.md. Never copy Windows icons, artwork or code from reference screenshots.
