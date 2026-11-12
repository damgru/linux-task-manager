# Task Manager for Linux

A native Rust desktop task manager with the layout and workflow of the Windows 11 Task Manager. The interface is independently drawn in egui; normal-mode readings come from the running Linux system. `--demo` uses fully fictional fixtures and disables system actions and persistence.

## Start

```sh
cargo build --release
./target/release/linux-task-manager
```

Use `--low-memory` to start with a smaller window, five-second sampling, no graphs, and no process history. It also limits Mesa software-renderer workers to one unless you set `LP_NUM_THREADS` yourself. The process view defaults to a parent/child tree, sorted by memory within each branch. Use **Tree view / List view** to switch layouts; search keeps matching processes and their ancestors visible even in folded branches. **Apps** and **Processes** are separate tabs: Apps shows desktop applications and their helpers, while Processes shows background processes. Application helpers remain in their parent’s tree. Desktop application identities and icons are discovered from installed XDG desktop entries (PNG and SVG); other processes keep a generic icon. Right-click a column heading (or choose **Columns**) to toggle columns. Drag headings to reorder them, or use the left/right buttons in **Columns**. The selected order is saved, including when Name is moved. Process and Details column selections are saved separately. Select a process and choose **End task**, or right-click for suspend/resume, command copying, and details. Force quit is available in the confirmation dialog. **Ctrl+F** focuses search and **Delete** opens End task.

## Features

- Separate **Apps** and **Processes** tabs for desktop applications (including their helpers) and background processes. Both support search, sorting, tree/list layouts and configurable columns; **Details** lists all processes.
- English and Polish interfaces; **Settings → Language** defaults to automatic detection from the Linux locale and applies immediately.
- CPU total and per-logical-processor graphs; physical memory, cache, and swap readings.
- Disk activity and throughput, network throughput, and available GPU telemetry. **Settings → Hide virtual network adapters** limits the Performance list to hardware-backed Wi-Fi and Ethernet adapters; this preference is saved and applies immediately. If a hidden adapter was selected, Performance selects an available physical adapter or CPU.
- Session process history, aggregated users, XDG startup entry toggles, system and user systemd services.
- Run new task, process termination, suspend/resume, and reduced scheduling priority.
- Persistent theme, start page, sampling speed, window-on-top, confirmation, language, process layout, and column preferences.
- Drag the title bar, double-click to maximize, or drag the lower-right corner to resize.

## Resource design

A single collector thread reads `/proc` and `/sys`; it does not launch `ps`, `top`, or a browser. The UI paints in response to input or a fresh sample. Samples use a replaceable latest-value slot rather than an accumulating queue. Graphs retain at most 60 samples per resource. Rows are virtualized; command lines are bounded. Old session history is pruned above 4096 process identities and is disabled in low memory mode. The release profile uses size optimization, thin LTO, and stripped symbols.

OpenGL driver memory contributes to the process's RAM use, particularly with software rendering. Startup in a completely exhausted system cannot be guaranteed. This version has not been validated in a real out-of-memory condition. Use the release binary for performance measurements. In this isolated Linux environment with software graphics, a short low-memory-mode measurement showed approximately 157 MiB RSS (93 MiB proportional memory, 67 MiB private pages) and 0.6% of one CPU core while idle. This is not an htop-sized memory footprint, and the value will vary by graphics driver and workload.

## Linux integration

The window uses a stable `linux-task-manager` application ID and an embedded icon. Normal startup registers a per-user XDG launcher and its icon under `$XDG_DATA_HOME` (default `~/.local/share`), so Ubuntu shows **Task Manager / Menedżer zadań** in the dock and application menu. Its executable path is refreshed if the binary moves. Screenshot and self-test runs do not register a launcher. On desktops with `DISPLAY`, the app uses X11 (XWayland on Ubuntu Wayland sessions) so **Always on top** works immediately and on subsequent launches. A pure Wayland session without XWayland does not support this window-level request.

Application trees start collapsed; background process trees start fully expanded. Each tab keeps its own manual expansion choices for the session. New applications also start collapsed, and search temporarily reveals matches inside folded branches.

Requires Linux 5.3+ for PID-safe process signals, a graphical X11/Wayland session, and an OpenGL-capable driver (software rendering also works). Process identity is checked and signals use a pidfd, preventing PID reuse from targeting another process. PID 1 and Task Manager itself are protected. The app runs with ordinary user permissions. If a process or system-service operation is denied, an authorization dialog offers **Authenticate…**. Polkit (`pkexec`) then opens the desktop authentication dialog and retries only that operation in a small, root-only helper entry point; the GUI stays unprivileged. The helper accepts an allowlist of process signals, nice +10, and system-service start/stop/restart actions. It rechecks process identity after authentication and never evaluates helper arguments in a shell. A working Polkit desktop agent is required; cancellation leaves the operation unapplied. User service access remains within the current user’s systemd session.

Memory columns report resident memory (RSS); totals by user can count shared pages more than once. Per-process CPU is normalized to total machine capacity. Per-process disk I/O can be hidden by kernel permissions. GPU telemetry follows the interfaces used by the installed Vitals extension: AMD DRM/sysfs counters and NVIDIA `nvidia-smi`. One persistent, read-only NVIDIA query reports utilization, VRAM, temperature, video encode/decode, power and driver version; devices are matched by PCI address. The subprocess is stopped when the collector closes. Unsupported counters show "Not reported"; per-process GPU/network/power columns show — when unavailable, with an explanation on hover. The GPU page uses real readings rather than inferred per-process values. In this isolated test session, `nvidia-smi` cannot communicate with the NVIDIA driver, so actual NVIDIA hardware readings and the desktop authentication round trip still require a normal desktop session.

App history is session-only. Startup toggles create user overrides in the XDG autostart directory and affect future desktop logins. Service controls use `systemctl` and require an available systemd bus and uses Polkit when system-service permissions need elevation. Settings are stored under `$XDG_CONFIG_HOME/linux-task-manager` (or `~/.config/linux-task-manager`).

## Verification

```sh
cargo test
./target/release/linux-task-manager --self-test
./target/release/linux-task-manager --demo --language en --screenshot /tmp/task-manager.png
./target/release/linux-task-manager --demo --language en --page performance --resource cpu --screenshot /tmp/cpu.png
```

The self-test reads live procfs and terminates only its own disposable `sleep` child. Screenshot mode opens without activation, keeps repainting while unfocused, captures after approximately 120 frames, and exits. `--page` accepts `apps`, `processes`, `details`, `services`, `settings`, `users`, `startup-apps`, and `app-history`; `--resource` accepts `cpu`, `memory`, `disk`, `network`, and `gpu` (prefers a device with telemetry, then NVIDIA). `--language auto|en|pl` overrides the language for that run. Automated tests also click tree chevrons, header column menus, the language selector and authorization cancellation without making system changes.

The supplied references are static Windows screenshots, so exact interactive parity cannot be verified. Windows-only metadata is replaced by relevant Linux data. The visual styling uses original vector shapes; no reference artwork is bundled.
