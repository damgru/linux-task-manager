# Changelog

This project uses semantic versions and records user-visible changes here. Releases are prepared manually on Linux.

## 0.0.2 — 2026-11-12

Changes since 0.0.1:

- Refresh the GitHub README with an animated tour of every page, all five Performance views and Application details, captured from English fictional demo data. Keep media as release attachments and add a reproducible capture script.
- Clearly mark the application experimental and use at your own risk, in English and Polish. Describe English/Polish interface support and language selection outside the main title.
- Document Application details, installation metadata, update checks and authorized network sampling in the README. Update download links, CLI/package version and AppStream release metadata to 0.0.2.

- Add a sortable **Total CPU** column (**CPU Razem**) to Apps and Processes, enabled by default for new and existing installations. Sum the process and all descendants once per sample, including folded or filtered-out subprocesses, using the same system-capacity percentage as CPU. Preserve subsequent column visibility and order preferences.

- Make End task the primary action on application details and add a navigable parent process with PID reuse checks.
- Add background update checks and an Update action when a package candidate is known (deb, Flatpak, Snap, dnf/rpm and pacman). System transactions use Polkit; user Flatpaks remain unprivileged. Cached sources and Arch full-system upgrades are explicitly explained.
- Make the Network cell clickable for a short Polkit-authorized NetHogs measurement. Label samples and expire them after 10 seconds; explain missing NetHogs, capture failures or unattributable traffic without inventing readings.

- Keep low-usage and unavailable resource cells neutral in Apps and Processes. Highlight only CPU above 5%, memory above 5% of system RAM, and disk I/O above 1 MiB/s, with stronger shading as usage rises.
- Show the system memory usage percentage above **Total memory**, matching **Memory**.
- Double-click a process/application for its resource summary, owner, elapsed time, bounded CPU/memory/disk graphs, and virtualized subprocess list. Return to the original list with Back or Alt+Left; Properties offers reduced priority and suspend/resume through existing safe process actions.
- Inspect installed package format and version on demand in a background worker: deb, Flatpak, Snap, rpm and pacman; recognize AppImage without guessing its version. Missing/denied information stays explicitly unavailable. Demo details remain synthetic and read-only.

- Resize columns by dragging header edges in every table; remember widths across restarts, independently for process/details and each other table. Reset columns also restores default widths.
- Animate hover, keyboard focus and pressed feedback on frameless toolbar and navigation buttons; disabled actions stay inactive.

- Add a persistent, multi-select **Settings → General → Footer** menu for process count, Linux load averages, CPU, GPU, memory and swap. Counters update immediately; existing installations keep their original three counters by default.
- Default to **Apps** on launch, while preserving an explicitly saved start page selected in **Settings → General**.
- Add a sortable **Total memory** column (**Pamięć Razem**) to Apps and Processes, enabled by default for new and existing installations. It sums the process's RSS and all descendants, including folded or filtered-out helpers; shared pages may be counted more than once.

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
