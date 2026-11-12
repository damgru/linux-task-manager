# Architecture

This is a single native Linux executable; there is no browser runtime or backend service.

| Module | Responsibility |
| --- | --- |
| main.rs | egui pages, navigation, settings and action dialogs |
| model.rs | procfs/sysfs sampling, bounded histories, PID-safe signals, XDG/systemd adapters |
| process_view.rs | Process trees, app classification and configurable columns |
| gpu.rs | AMD sysfs telemetry and one persistent read-only NVIDIA query |
| desktop.rs | XDG application identities and lazy icon decoding |
| privilege.rs | Polkit authentication and root helper argument allowlist |
| integration.rs | Stable app/window identity and per-user launcher |
| i18n.rs | English/Polish translation |
| demo.rs | Entirely synthetic, read-only public demonstration data |

A collector thread publishes only its latest snapshot through a replaceable slot. The UI repaints on input or fresh data and virtualizes process rows. Each resource retains at most 60 samples. Process history is session-only, pruned above 4096 identities, and disabled in low-memory mode. The release profile favors binary size.

The GUI never becomes root. Process actions verify PID/start time and use pidfd. Permission failures can offer a separate Polkit helper invocation for one allowlisted action. Service/startup access uses systemd/XDG conventions.

Demo mode creates an idle monitor containing fixtures, bypasses service/startup enumeration and settings/launcher persistence, disables toolbar system controls, and guards process actions and dialogs. It must stay isolated from the real collector even if a screenshot page changes. Normal operation continues to use live Linux data.
