//! Desktop metadata is discovered once; only icons of running applications are
//! decoded, on the collector thread. No desktop command is ever executed.
use eframe::egui::ColorImage;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Debug)]
pub struct DesktopApp {
    pub id: String,
    pub icon: Option<Arc<ColorImage>>,
}
#[derive(Default)]
pub struct Catalog {
    aliases: HashMap<String, (String, String)>,
    icons: HashMap<String, PathBuf>,
    loaded: HashMap<String, Arc<DesktopApp>>,
}
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"))];
    dirs.extend(std::env::split_paths(&std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into())));
    dirs
}
fn files(dir: &Path, depth: usize, visit: &mut impl FnMut(PathBuf)) {
    if depth == 0 {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                files(&entry.path(), depth - 1, visit);
            } else if entry.path().is_file() {
                visit(entry.path());
            }
        }
    }
}
fn basename(value: &str) -> String {
    Path::new(value).file_name().unwrap_or_default().to_string_lossy().to_lowercase()
}
fn executable(exec: &str) -> Option<String> {
    // Exec is only metadata. Respect quoted executable paths and skip env
    // assignments without interpreting shell substitutions or field codes.
    let words = tokens(exec);
    let mut iter = words.iter();
    let mut word = iter.next()?;
    if basename(word) == "env" {
        word = iter.find(|v| !v.contains('=') && !v.starts_with('-'))?;
    }
    Some(word.clone())
}
fn tokens(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in value.chars() {
        if escaped {
            token.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !token.is_empty() {
                result.push(std::mem::take(&mut token));
            }
        } else {
            token.push(c);
        }
    }
    if !token.is_empty() {
        result.push(token);
    }
    result
}
impl Catalog {
    pub fn load() -> Self {
        let mut result = Self::default();
        let dirs = data_dirs();
        let mut seen = HashSet::new();
        for dir in &dirs {
            files(&dir.join("applications"), 3, &mut |path| {
                if path.extension().is_none_or(|e| e != "desktop") {
                    return;
                }
                let id = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                if !seen.insert(id.clone()) {
                    return;
                }
                let Ok(contents) = fs::read_to_string(&path) else {
                    return;
                };
                let fields = desktop_fields(&contents);
                if fields.get("Type").copied() != Some("Application")
                    || ["Hidden", "NoDisplay", "Terminal"].iter().any(|key| fields.get(key).copied() == Some("true"))
                {
                    return;
                }
                let Some(exec) = fields.get("Exec").and_then(|v| executable(v)) else {
                    return;
                };
                let icon = fields.get("Icon").copied().unwrap_or_default().to_owned();
                let mut aliases = vec![basename(&exec), id.to_lowercase(), id.rsplit('.').next().unwrap_or(&id).to_lowercase()];
                if let Some(wm) = fields.get("StartupWMClass") {
                    aliases.push(wm.to_lowercase());
                    aliases.push(wm.rsplit('-').next().unwrap_or(wm).to_lowercase());
                }
                if let Ok(real) = fs::canonicalize(&exec) {
                    aliases.push(basename(&real.to_string_lossy()));
                }
                for alias in aliases {
                    if !["env", "sh", "bash", "python", "python3", "flatpak", "snap", "java"].contains(&alias.as_str()) && !alias.is_empty() {
                        result.aliases.entry(alias).or_insert_with(|| (id.clone(), icon.clone()));
                    }
                }
            });
        }
        let wanted: HashSet<_> = result.aliases.values().map(|(_, icon)| icon.as_str()).collect();
        let mut roots: Vec<_> = dirs.iter().map(|d| d.join("icons")).collect();
        if let Some(home) = std::env::var_os("HOME") {
            roots.insert(0, PathBuf::from(home).join(".icons"));
        }
        roots.extend(dirs.iter().map(|d| d.join("pixmaps")));
        for root in roots {
            files(&root, 6, &mut |path| {
                if !matches!(path.extension().and_then(|v| v.to_str()), Some("png" | "svg")) {
                    return;
                }
                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                if wanted.contains(name.as_ref()) {
                    let keep = result.icons.get(name.as_ref()).is_none_or(|old| icon_score(&path) < icon_score(old));
                    if keep {
                        result.icons.insert(name.into_owned(), path);
                    }
                }
            });
        }
        result
    }
    pub fn find(&mut self, name: &str, command: &str) -> Option<Arc<DesktopApp>> {
        // Kernel comm is limited to 15 bytes; prefer the full command executable.
        let command = executable(command).map(|s| basename(&s)).unwrap_or_default();
        let key = self.aliases.get(&command).or_else(|| self.aliases.get(&name.to_lowercase()))?;
        let (id, icon) = key;
        if let Some(app) = self.loaded.get(id) {
            return Some(app.clone());
        }
        let path = if Path::new(icon).is_absolute() { Some(PathBuf::from(icon)) } else { self.icons.get(icon).cloned() };
        let app = Arc::new(DesktopApp { id: id.clone(), icon: path.as_deref().and_then(load_icon).map(Arc::new) });
        self.loaded.insert(id.clone(), app.clone());
        Some(app)
    }
}
fn desktop_fields(text: &str) -> HashMap<&str, &str> {
    let mut active = false;
    let mut fields = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            active = line == "[Desktop Entry]";
        } else if active && !line.starts_with('#') {
            if let Some((key, value)) = line.split_once('=') {
                fields.insert(key, value);
            }
        }
    }
    fields
}
fn icon_score(path: &Path) -> u8 {
    let text = path.to_string_lossy();
    if text.contains("/32x32/") {
        0
    } else if text.contains("/48x48/") {
        1
    } else if text.contains("/scalable/") {
        2
    } else {
        3
    }
}
fn load_icon(path: &Path) -> Option<ColorImage> {
    // Bound input and output independently of an icon's advertised dimensions.
    if fs::metadata(path).ok()?.len() > 2 * 1024 * 1024 {
        return None;
    }
    if path.extension()?.to_str()? == "svg" {
        let tree = resvg::usvg::Tree::from_data(&fs::read(path).ok()?, &resvg::usvg::Options::default()).ok()?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(32, 32)?;
        let scale = 32.0 / tree.size().width().max(tree.size().height());
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        let pixels =
            pixmap.pixels().iter().map(|p| eframe::egui::Color32::from_rgba_premultiplied(p.red(), p.green(), p.blue(), p.alpha())).collect();
        Some(ColorImage { size: [32, 32], pixels })
    } else {
        let mut reader = image::ImageReader::open(path).ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(2048);
        limits.max_image_height = Some(2048);
        limits.max_alloc = Some(16 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().ok()?.thumbnail(32, 32).to_rgba8();
        Some(ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_and_svg_icons_are_decoded_to_small_textures() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("task-manager-icons-{}-{unique}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let svg = dir.join("app.svg");
        fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#ff0000"/></svg>"##)
            .unwrap();
        let decoded = load_icon(&svg).unwrap();
        assert_eq!(decoded.size, [32, 32]);
        assert_eq!(decoded.pixels[0], eframe::egui::Color32::RED);
        let png = dir.join("app.png");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 255, 0, 255])).save(&png).unwrap();
        let decoded = load_icon(&png).unwrap();
        assert_eq!(decoded.size, [32, 32]);
        assert_eq!(decoded.pixels[0], eframe::egui::Color32::GREEN);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn desktop_actions_do_not_override_main_entry() {
        let fields = desktop_fields(
            "[Desktop Entry]\nType=Application\nExec=\"/opt/My App/tool\" %U\nIcon=my-icon\n[Desktop Action test]\nExec=wrong\nIcon=wrong",
        );
        assert_eq!(executable(fields["Exec"]).as_deref(), Some("/opt/My App/tool"));
        assert_eq!(fields["Icon"], "my-icon");
        assert_eq!(executable("env FLAG=1 /usr/bin/thing %U").as_deref(), Some("/usr/bin/thing"));
    }
}
