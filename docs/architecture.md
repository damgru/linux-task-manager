# Architecture

This is a single native Linux executable; there is no browser runtime or backend service.

| Module | Responsibility |
| --- | --- |
| application_page.rs | Application summaries, bounded detail histories, subprocess UI and properties |
| app_details.rs | On-demand package metadata and process-tree traversal |
| main.rs | egui pages, navigation, settings and action dialogs |
| model.rs | procfs/sysfs sampling, bounded histories, PID-safe signals, XDG/systemd adapters |
| process_view.rs | Process trees, app classification and configurable columns |
| gpu.rs | AMD sysfs telemetry and one persistent read-only NVIDIA query |
| desktop.rs | XDG application identities and lazy icon decoding |
| privilege.rs | Polkit authentication and root helper argument allowlist |
| updates.rs / commands.rs | Package candidate queries, allowlisted updates, bounded command output |
| process_network.rs | Short NetHogs sampling with process identity validation |
| integration.rs | Stable app/window identity and per-user launcher |
| i18n.rs | English/Polish translation |
| docker.rs | Bounded Docker CLI JSON inventory, statistics, Compose grouping, image layers, logs and ID-only lifecycle actions |
| docker_page.rs | Lazy Docker background requests, single pending snapshots, views, bounded detail histories and confirmations |
| demo.rs | Entirely synthetic, read-only public demonstration data |

A collector thread publishes only its latest snapshot through a replaceable slot. The UI repaints on input or fresh data and virtualizes process rows. Each resource retains at most 60 samples. Process history is session-only, pruned above 4096 identities, and disabled in low-memory mode. The release profile favors binary size.

CPU and RSS subtree totals share a single iterative traversal per collected snapshot, independent of filtering or folding in the UI. Missing, self-linked or reused parents form roots; cycles count each reachable process once. Demo fixtures use the same aggregation.

The GUI never becomes root. Process actions verify PID/start time and use pidfd. Permission failures can offer a separate Polkit helper invocation for one allowlisted action. Service/startup access uses systemd/XDG conventions.

Demo mode creates an idle monitor containing fixtures, bypasses service/startup enumeration and settings/launcher persistence, disables toolbar system controls, and guards process actions and dialogs. It must stay isolated from the real collector even if a screenshot page changes. Normal operation continues to use live Linux data.

Docker detection is asynchronous and never runs in demo mode. Inventory is collected only while the Docker page is visible, at most once per five seconds (ten in low-memory mode); pause/manual refresh follows the existing update setting. One in-flight collection and one bounded response channel prevent queued snapshots. Detail histories keep 60 samples and disappear when navigating away. Image/container inspection is batched in groups of four; inventory is capped at 256 unique containers/images per kind, command output at 64 KiB per pipe and queries at 30 seconds. Exceeding a bound is an explicit error, never silently incomplete inventory. Stats failures retain inventory with unavailable counters. Collection failures mark old data stale and disable controls.

Docker uses the current CLI context (or explicit DOCKER_CONTEXT/DOCKER_HOST), captured at detection and pinned for reads/actions. All lifecycle actions accept only validated full immutable container IDs and a fixed start/stop/restart/kill/rm allowlist. Container rm optionally uses --force only after explicit confirmation; image rm accepts full image IDs without force. Both deletions always require confirmation and preserve volumes. Arguments are separated from IDs with --. Project actions capture the displayed member IDs at confirmation; no Compose file, shell, arbitrary executable or root helper is run. SIGKILL always requires confirmation. Access errors are reported using current user permissions. Bounded log queries include both stdout and stderr, at most 40 lines from each of up to eight members and 48,000 characters total; no follow stream is started. Demo mode does not even detect the CLI and guards every action independently.

Docker column visibility/order is stored separately for container and image tables using validated heading keys; widths use the existing bounded width map. Embedded container tables share the container layout. Docker emits preference-change notifications to the normal app settings handler, preserving demo/screenshot isolation and save-error handling. Resource summaries use unique snapshot containers rather than rendered project/child rows, so search and folding cannot alter totals.
