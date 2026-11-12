# Linux Task Manager

A native Linux task manager with a Windows-inspired interface, written in Rust with egui. Monitor applications and background processes, inspect CPU/memory/disk/network/GPU activity, and manage tasks without a browser runtime.

**Experimental · Linux only · MIT licensed**

> **Warning: this application is experimental. Use it entirely at your own risk.** It can terminate processes and change system settings or installed packages. Bugs may cause data loss or disrupt your session. Save your work before using system actions.
>
> **Uwaga: aplikacja jest eksperymentalna. Korzystasz z niej wyłącznie na własne ryzyko!** Może kończyć procesy oraz zmieniać ustawienia systemu i zainstalowane pakiety. Błędy mogą spowodować utratę danych lub zakłócić sesję. Zapisz pracę przed wykonaniem działań systemowych.

[Download 0.0.2](https://github.com/damgru/linux-task-manager/releases/tag/v0.0.2) · [Report a bug](https://github.com/damgru/linux-task-manager/issues) · [Contribute](CONTRIBUTING.md) · [Changelog](CHANGELOG.md)

![Animated tour: Apps, Processes, CPU, Memory, Disk, Network, GPU, App history, Startup apps, Users, Details, Services, Settings and Application details — fictional demonstration data](https://github.com/damgru/linux-task-manager/releases/download/v0.0.2/demo-tour.gif)

All public screenshots use `--demo`: process names, users, hardware and readings are fictional. Normal operation reads your Linux system. Generated media are release attachments, not source files.

## Languages

The interface supports **English and Polish**. Language is detected from your Linux locale; change it immediately in **Settings → Language**, or start with `--language en` / `--language pl`.

Interfejs obsługuje **język polski i angielski**. Język jest wykrywany na podstawie ustawień systemu; możesz go zmienić w **Ustawienia → Język** lub uruchomić program z `--language pl` / `--language en`.

## Download and run

Version **0.0.2** provides **Linux x86_64** packages. It requires Linux 5.3+, glibc 2.36+ (Debian 12 / Ubuntu 24.04 or newer), X11 or Wayland, and an OpenGL-capable driver. No macOS or Windows builds are published. ARM64 is not yet validated.

| Package | Install / run |
| --- | --- |
| [Debian / Ubuntu .deb](https://github.com/damgru/linux-task-manager/releases/download/v0.0.2/linux-task-manager-0.0.2-linux-x86_64.deb) | `sudo apt install ./linux-task-manager-0.0.2-linux-x86_64.deb` |
| [AppImage](https://github.com/damgru/linux-task-manager/releases/download/v0.0.2/linux-task-manager-0.0.2-linux-x86_64.AppImage) | Mark executable, then double-click or run it |
| [Portable tar.gz](https://github.com/damgru/linux-task-manager/releases/download/v0.0.2/linux-task-manager-0.0.2-linux-x86_64.tar.gz) | Extract and run `./linux-task-manager` |

Verify downloads using [SHA256SUMS.txt](https://github.com/damgru/linux-task-manager/releases/download/v0.0.2/SHA256SUMS.txt):

```sh
sha256sum -c SHA256SUMS.txt --ignore-missing
```

Packages are unsigned. AppImage needs working FUSE mounting; if unavailable, run `./linux-task-manager-0.0.2-linux-x86_64.AppImage --appimage-extract-and-run`. It still uses your system graphics libraries and driver. The portable binary requires the same X11/Wayland/OpenGL libraries; see [runtime prerequisites](CONTRIBUTING.md#runtime-libraries).

Launch `linux-task-manager` from the application menu after installing the .deb. Portable and AppImage launches register a per-user menu entry on first normal startup. Run as an ordinary user; a desktop Polkit agent and `pkexec` are needed only for operations requiring administrator permissions. NVIDIA telemetry additionally uses `nvidia-smi` when available.

## Features

- Separate **Apps** and **Processes** trees, search, sorting, resizable/reorderable columns, and sortable **Total CPU / Total memory** columns including every descendant.
- Dedicated **Application details** pages with resource graphs, subprocesses, installation metadata and package update checks.
- CPU/per-core graphs, memory and swap, disks, hardware/virtual network adapters, and available AMD/NVIDIA GPU counters.
- End task, force quit, suspend/resume and lower scheduling priority, with PID reuse protection.
- Session app history, users, XDG startup controls and system/user systemd services.
- Persistent themes, sampling interval, language, columns, configurable footer counters and default start page; always-on-top through X11/XWayland.
- `--low-memory`: five-second refresh, no graphs or process history, fewer software-renderer workers.

![CPU graphs with fictional hardware and telemetry](https://github.com/damgru/linux-task-manager/releases/download/v0.0.2/demo-cpu.png)

The UI uses bounded histories and virtualized rows, but OpenGL driver overhead is significant: an earlier software-renderer measurement was about 157 MiB RSS in low-memory mode. This is not an htop-sized memory footprint, and real out-of-memory startup has not been validated. Unavailable counters are marked rather than inferred. Always-on-top needs X11/XWayland; pure Wayland support depends on the compositor. See [full usage and limitations](docs/usage.md).

## Application details

Double-click an application or process, or choose **Application details** from its context menu. Inspect its PID, owner, parent process, elapsed time, threads, priority, CPU/memory/disk usage and short resource graphs. Browse and search its subprocesses, navigate to its parent, or use **Back / Alt+Left** to return to the list. **End task** is the primary action; **Properties** offers suspend/resume and reduced priority.

Installation details show the package format and installed version when available (deb, Flatpak, Snap, rpm and pacman; AppImage detection without guessing a version). Check for updates in the background and install a known candidate through the package manager. System changes request Polkit authorization; user Flatpak updates stay unprivileged. Arch updates require a full system upgrade. Missing information is explained rather than invented.

Click a **Network** cell for a short, authorized NetHogs measurement. Install NetHogs separately; samples expire after 10 seconds and are explicitly marked. See [application details and limitations](docs/usage.md#application-details).

**Szczegóły aplikacji:** kliknij dwukrotnie aplikację lub proces, aby zobaczyć zużycie zasobów, wykresy, podprocesy, proces nadrzędny oraz informacje o instalacji i dostępnych aktualizacjach. Działania systemowe mogą wymagać uwierzytelnienia Polkit.

## Build and check

Use Rust 1.93.1+ and the [Linux development libraries](CONTRIBUTING.md#develop-on-linux):

```sh
cargo run --release --locked
scripts/check.sh
```

Checks run locally: formatting, Clippy with warnings denied, tests, release build and a self-test that terminates only its own disposable child. **There is no GitHub Actions pipeline.** Official packages use the pinned Debian 12 builder for glibc compatibility; see [manual releases](docs/releasing.md).

## Safe screenshots

```sh
cargo run --release --locked -- --demo --language en --screenshot /tmp/demo-processes.png
./target/release/linux-task-manager --demo --language en --page performance --resource cpu --screenshot /tmp/demo-cpu.png
```

Demo mode never starts the live collector, enumerates real services/startup entries, writes preferences or registers a launcher. System-changing controls are disabled. Save media outside the checkout and attach it to a release or PR.

Regenerate the complete README tour with `scripts/capture-demo.sh /tmp/ltm-demo-tour` (requires FFmpeg). It captures every page, all five Performance resource views and Application details in English demo mode, then creates `demo-tour.gif` outside the checkout.

See [architecture](docs/architecture.md), [working rules](AGENTS.md), [roadmap](ROADMAP.md) and [security policy](SECURITY.md).

## License and credits

[MIT](LICENSE). Original UI and SVG artwork; no Windows assets or code are bundled. Independent project, not affiliated with Microsoft. [Attribution](ATTRIBUTION.md) and [third-party notices](THIRD-PARTY-LICENSES.txt) cover dependencies and embedded fonts. Repository maintenance conventions were informed by [SolidCraft](https://github.com/tangency-labs/solidcraft).
