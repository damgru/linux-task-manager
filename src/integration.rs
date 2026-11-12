//! Window identity and per-user desktop registration share one stable ID.
use eframe::egui::IconData;
use std::{fs, io, path::Path};

pub const APP_ID: &str = "linux-task-manager";
const ICON: &str = include_str!("../assets/linux-task-manager.svg");

pub fn window_icon() -> IconData {
    let tree = resvg::usvg::Tree::from_str(ICON, &resvg::usvg::Options::default()).expect("bundled icon");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(128, 128).expect("icon size");
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    IconData { rgba, width: 128, height: 128 }
}

// Exec quoting is applied after the desktop file's string unescaping.
fn exec_path(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '\\' | '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                if c == '\\' {
                    quoted.push_str("\\\\");
                } else {
                    quoted.push(c);
                }
            }
            '%' => quoted.push_str("%%"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn write_changed(path: &Path, text: &str) -> io::Result<()> {
    if fs::read_to_string(path).ok().as_deref() != Some(text) {
        fs::create_dir_all(path.parent().expect("desktop directory"))?;
        fs::write(path, text)?;
    }
    Ok(())
}

fn register_at(data: &Path, exe: &Path) -> io::Result<()> {
    let exe = exe.to_str().ok_or_else(|| io::Error::other("executable path is not UTF-8"))?;
    write_changed(&data.join(format!("icons/hicolor/scalable/apps/{APP_ID}.svg")), ICON)?;
    write_changed(
        &data.join(format!("applications/{APP_ID}.desktop")),
        &format!("[Desktop Entry]\nType=Application\nName=Task Manager\nName[pl]=Menedżer zadań\nComment=Monitor applications, processes and system resources\nExec={}\nIcon={APP_ID}\nStartupWMClass={APP_ID}\nTerminal=false\nCategories=System;Monitor;\n", exec_path(exe)),
    )
}

pub fn register_desktop() -> io::Result<()> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")))
        .ok_or_else(|| io::Error::other("no user data directory"))?;
    // AppImage's mounted current_exe disappears when the app exits. The runtime
    // supplies the persistent outer file path in APPIMAGE instead.
    let exe = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from).filter(|p| p.is_absolute() && p.is_file());
    register_at(&data, &exe.unwrap_or(std::env::current_exe()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launcher_and_window_share_identity_and_icon() {
        let dir = std::env::temp_dir().join(format!("task-manager-desktop-{}", std::process::id()));
        register_at(&dir, Path::new("/tmp/Task Manager/linux-task-manager")).unwrap();
        let desktop = fs::read_to_string(dir.join(format!("applications/{APP_ID}.desktop"))).unwrap();
        assert!(desktop.contains("Exec=\"/tmp/Task Manager/linux-task-manager\"\n"));
        assert!(desktop.contains(&format!("Icon={APP_ID}\nStartupWMClass={APP_ID}\n")));
        assert!(dir.join(format!("icons/hicolor/scalable/apps/{APP_ID}.svg")).exists());
        let icon = window_icon();
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert!(icon.rgba.chunks_exact(4).any(|p| p[3] == 255));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn desktop_exec_preserves_special_characters() {
        assert_eq!(exec_path("/tmp/a\\b\"$`%/app"), r#""/tmp/a\\\\b\\"\\$\\`%%/app""#);
    }
}
