# Attribution

| Material | Origin | License |
| --- | --- | --- |
| assets/linux-task-manager.svg | Original project SVG artwork | MIT, see LICENSE |
| Interface shapes/icons | Original Rust drawing code in src/main.rs | MIT |
| Demo process names and telemetry | Original fictional fixtures in src/demo.rs | MIT |
| AppImage type-2 runtime (AppImage only) | AppImage/type2-runtime, commit fedfad51acc2860b2916a1eef32ba4a500ff31ef | MIT with bundled-library terms; see packaging/APPIMAGE-RUNTIME-LICENSE.txt and its linked sources |
| egui bundled fonts | epaint_default_fonts: Hack, Ubuntu Light, Noto Emoji, emoji-icon-font | MIT / Ubuntu Font License 1.0 / SIL OFL 1.1; notices in THIRD-PARTY-LICENSES.txt |

The interface is inspired by Windows Task Manager. No Microsoft artwork, icons, screenshots or code are distributed. This project is independent and is not affiliated with Microsoft.

Cargo dependencies and their bundled license notices are listed in [THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt), generated from Cargo.lock using `python3 scripts/licenses.py`. The list includes transitive and platform-specific packages resolved by Cargo, some of which are not linked into the Linux executable. System libraries and optional external tools remain provided and licensed by the Linux distribution.

Repository maintenance conventions were informed by [SolidCraft](https://github.com/tangency-labs/solidcraft); its application code and artwork are not used.
