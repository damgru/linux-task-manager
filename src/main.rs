mod app_details;
mod application_page;
mod commands;
mod demo;
mod desktop;
mod docker;
mod docker_page;
mod gpu;
mod integration;
mod model;
mod privilege;
mod process_network;
#[cfg(test)]
mod ui_tests;
mod updates;
use privilege::{permission_denied, PrivilegedAction, ProcessAction};
mod i18n;
mod process_view;
use eframe::egui::{self, epaint::PathShape, *};
use i18n::{tr, Language};
use model::*;
use process_view::Column;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{atomic::Ordering, mpsc, Arc},
    time::{Duration, Instant},
};
const ACCENT: Color32 = Color32::from_rgb(244, 142, 117);
const CYAN: Color32 = Color32::from_rgb(75, 159, 187);
const BLUE: Color32 = Color32::from_rgb(96, 156, 218);
const PINK: Color32 = Color32::from_rgb(198, 93, 132);
const PURPLE: Color32 = Color32::from_rgb(175, 102, 193);
const GREEN: Color32 = Color32::from_rgb(123, 168, 103);
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Apps,
    Processes,
    Performance,
    History,
    Startup,
    Users,
    Details,
    Services,
    Docker,
    Settings,
}
impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Apps => "Apps",
            Self::Processes => "Processes",
            Self::Performance => "Performance",
            Self::History => "App history",
            Self::Startup => "Startup apps",
            Self::Users => "Users",
            Self::Details => "Details",
            Self::Services => "Services",
            Self::Docker => "Docker",
            Self::Settings => "Settings",
        }
    }
    fn all() -> [Self; 10] {
        [
            Self::Apps,
            Self::Processes,
            Self::Performance,
            Self::History,
            Self::Startup,
            Self::Users,
            Self::Details,
            Self::Services,
            Self::Docker,
            Self::Settings,
        ]
    }
    fn icon(self) -> usize {
        match self {
            Self::Apps => 0,
            Self::Processes | Self::Details => 5,
            Self::Performance => 1,
            Self::History => 2,
            Self::Startup => 3,
            Self::Users => 4,
            Self::Services => 6,
            Self::Docker => 10,
            Self::Settings => 7,
        }
    }
    fn from(s: &str) -> Self {
        Self::all().into_iter().find(|p| p.title().to_lowercase().replace(' ', "-") == s).unwrap_or(Self::Apps)
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Resource {
    Cpu,
    Memory,
    Disk(usize),
    Network(usize),
    Gpu(usize),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Cpu,
    TotalCpu,
    Memory,
    TotalMemory,
    Disk,
    Pid,
    Parent,
    User,
    Command,
    Threads,
    Priority,
    Read,
    Write,
    Status,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FooterItem {
    Processes,
    Load,
    Cpu,
    Gpu,
    Memory,
    Swap,
}
impl FooterItem {
    fn all() -> [Self; 6] {
        [Self::Processes, Self::Load, Self::Cpu, Self::Gpu, Self::Memory, Self::Swap]
    }
    fn key(self) -> &'static str {
        match self {
            Self::Processes => "processes",
            Self::Load => "load",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Memory => "memory",
            Self::Swap => "swap",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Processes => "Process count",
            Self::Load => "Load",
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
            Self::Memory => "Memory",
            Self::Swap => "Swap",
        }
    }
    fn value(self, data: &Snapshot) -> String {
        match self {
            Self::Processes => format!("{} {}", data.processes.len(), tr("Processes")),
            Self::Load => data.load.map(|[a, b, c]| format!("Load {a:.2} / {b:.2} / {c:.2}")).unwrap_or_else(|| "Load —".into()),
            Self::Cpu => format!("CPU {:.0}%", data.cpu),
            Self::Gpu => data.gpus.iter().filter_map(|g| g.busy).reduce(f32::max).map(|v| format!("GPU {v:.0}%")).unwrap_or_else(|| "GPU —".into()),
            Self::Memory => format!("{} {} / {}", tr("Memory"), bytes(data.mem_used()), bytes(data.mem_total)),
            Self::Swap => format!("Swap {} / {}", bytes(data.swap_total.saturating_sub(data.swap_free)), bytes(data.swap_total)),
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Processes => "Total number of system processes",
            Self::Load => "Linux load averages over 1, 5 and 15 minutes; not a percentage. — means not reported.",
            Self::Gpu => "Highest reported GPU utilization. — means not reported by the driver.",
            Self::Cpu => "CPU is shown as a percentage of total system capacity.",
            Self::Memory => "Used / total physical memory",
            Self::Swap => "Used / total swap; 0 KB / 0 KB means no swap is configured",
        }
    }
}
struct Settings {
    theme: String,
    start: Page,
    interval: u64,
    always_top: bool,
    low_memory: bool,
    confirm: bool,
    language: Language,
    tree: bool,
    columns: Vec<Column>,
    detail_columns: Vec<Column>,
    docker_columns: Vec<usize>,
    docker_image_columns: Vec<usize>,
    hide_virtual_networks: bool,
    footer: Vec<FooterItem>,
    column_widths: HashMap<String, f32>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "Dark".into(),
            start: Page::Apps,
            interval: 1000,
            always_top: false,
            low_memory: false,
            confirm: true,
            language: Language::Auto,
            tree: true,
            columns: Column::defaults(),
            detail_columns: Column::details(),
            docker_columns: docker_page::default_columns(false),
            docker_image_columns: docker_page::default_columns(true),
            hide_virtual_networks: false,
            footer: vec![FooterItem::Processes, FooterItem::Cpu, FooterItem::Memory],
            column_widths: HashMap::new(),
        }
    }
}
impl Settings {
    fn load() -> Self {
        if demo::enabled() {
            return Self::default();
        }
        fs::read_to_string(config_dir().join("linux-task-manager/settings")).map(|text| Self::parse(&text)).unwrap_or_default()
    }
    fn parse(text: &str) -> Self {
        let mut s = Self::default();
        let mut columns_version = 1;
        for l in text.lines() {
            if let Some((k, v)) = l.split_once('=') {
                match k {
                    "theme" => s.theme = v.into(),
                    "start" => s.start = Page::from(v),
                    "interval" => s.interval = v.parse().unwrap_or(1000),
                    "top" => s.always_top = v == "true",
                    "low" => s.low_memory = v == "true",
                    "confirm" => s.confirm = v == "true",
                    "language" => s.language = Language::from(v),
                    "hide_virtual_networks" => s.hide_virtual_networks = v == "true",
                    "tree" => s.tree = v != "false",
                    "detail_columns" => s.detail_columns = Column::parse_list(v),
                    "columns" => s.columns = Column::parse_list(v),
                    "docker_columns" => s.docker_columns = docker_page::parse_columns(v, false),
                    "docker_image_columns" => s.docker_image_columns = docker_page::parse_columns(v, true),
                    "columns_version" => columns_version = v.parse().unwrap_or(1),
                    "column_widths" => {
                        for entry in v.split('|').take(256) {
                            if let Some((key, width)) = entry.rsplit_once(':') {
                                if let Ok(width) = width.parse::<f32>() {
                                    if width.is_finite() && (48.0..=2000.0).contains(&width) && key.len() <= 100 {
                                        s.column_widths.insert(key.into(), width);
                                    }
                                }
                            }
                        }
                    }
                    "footer" => s.footer = FooterItem::all().into_iter().filter(|item| v.split(',').any(|key| key == item.key())).collect(),
                    _ => {}
                }
            }
        }
        // Show the new column once for existing installations, then respect
        // subsequent hide/reorder choices saved by this version.
        if columns_version < 2 && !s.columns.contains(&Column::TotalMemory) {
            let index = s.columns.iter().position(|c| *c == Column::Memory).map_or(s.columns.len(), |i| i + 1);
            s.columns.insert(index, Column::TotalMemory);
        }
        if columns_version < 3 && !s.columns.contains(&Column::TotalCpu) {
            let index = s.columns.iter().position(|c| *c == Column::Cpu).map_or(s.columns.len(), |i| i + 1);
            s.columns.insert(index, Column::TotalCpu);
        }
        s
    }
    fn save(&self) -> Result<(), String> {
        if demo::enabled() {
            return Ok(());
        }
        let d = config_dir().join("linux-task-manager");
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        fs::write(d.join("settings"), self.serialize()).map_err(|e| e.to_string())
    }
    fn serialize(&self) -> String {
        let mut widths: Vec<_> = self.column_widths.iter().collect();
        widths.sort_by(|a, b| a.0.cmp(b.0));
        format!(
                "theme={}\nstart={}\ninterval={}\ntop={}\nlow={}\nconfirm={}\nlanguage={}\ntree={}\ncolumns={}\ndetail_columns={}\nhide_virtual_networks={}\ncolumns_version=3\nfooter={}\ncolumn_widths={}\ndocker_columns={}\ndocker_image_columns={}\n",
                self.theme,
                self.start.title().to_lowercase().replace(' ', "-"),
                self.interval,
                self.always_top,
                self.low_memory,
                self.confirm,
                self.language.key(),
                self.tree,
                self.columns.iter().map(|c| c.key()).collect::<Vec<_>>().join(","),
                self.detail_columns.iter().map(|c| c.key()).collect::<Vec<_>>().join(","),
                self.hide_virtual_networks,
                self.footer.iter().map(|item| item.key()).collect::<Vec<_>>().join(","),
                widths.iter().map(|(key, width)| format!("{key}:{width}")).collect::<Vec<_>>().join("|"),
                docker_page::serialize_columns(&self.docker_columns, false),
                docker_page::serialize_columns(&self.docker_image_columns, true)
        )
    }
}
#[derive(Clone)]
struct HistoryRow {
    name: String,
    pid: u32,
    cpu_seconds: f64,
    read: f64,
    write: f64,
    last_ticks: u64,
    last_seen: Instant,
}
enum Worker {
    Application((u32, u64), app_details::Metadata),
    UpdateCheck((u32, u64), Result<Option<updates::Available>, String>),
    Updated((u32, u64), Result<(), String>),
    Network((u32, u64), Result<f32, String>),
    Services(Vec<Service>, String),
    Startups(Vec<Startup>),
    Action(Result<(), String>),
    Authorize(PrivilegedAction),
}
struct TaskManager {
    docker: docker_page::DockerView,
    application: Option<application_page::ApplicationView>,
    properties: Option<Process>,
    metadata_busy: bool,
    network_samples: HashMap<(u32, u64), (f32, Instant)>,
    application_request: Option<u32>,
    monitor: Monitor,
    data: Arc<Snapshot>,
    page: Page,
    resource: Resource,
    collapsed: bool,
    search: String,
    sort: Sort,
    descending: bool,
    selected: Option<(u32, u64)>,
    kill_dialog: Option<Process>,
    force: bool,
    run_dialog: bool,
    command: String,
    message: Option<(String, bool, Instant)>,
    settings: Settings,
    services: Vec<Service>,
    services_error: String,
    service_selected: Option<usize>,
    startups: Vec<Startup>,
    startup_selected: Option<usize>,
    busy: bool,
    tx: mpsc::Sender<Worker>,
    rx: mpsc::Receiver<Worker>,
    last_services: Instant,
    history: HashMap<(u32, u64), HistoryRow>,
    session: Instant,
    logical: bool,
    screenshot: Option<PathBuf>,
    frames: u32,
    screenshot_requested: bool,
    folded: HashSet<(u32, u64)>,
    expanded_apps: HashSet<(u32, u64)>,
    authorization: Option<PrivilegedAction>,
    app_icons: HashMap<String, TextureHandle>,
}
fn button_feedback(ui: &Ui, response: &Response) -> Shape {
    let enabled = response.enabled();
    let hovered = enabled && (response.hovered() || response.has_focus());
    let hover = ui.ctx().animate_bool_with_time(response.id.with("hover-fill"), hovered, 0.12);
    let pressed = enabled && response.is_pointer_button_down_on();
    let press = ui.ctx().animate_bool_with_time(response.id.with("press-fill"), pressed, 0.06);
    let color = if ui.visuals().dark_mode { Color32::WHITE } else { Color32::BLACK };
    Shape::rect_filled(response.rect, 4.0, color.gamma_multiply(if enabled { 0.09 * hover + 0.10 * press } else { 0.0 }))
}
fn quiet_button(label: &str) -> impl Widget + '_ {
    move |ui: &mut Ui| {
        let background = ui.painter().add(Shape::Noop);
        let response = ui.add(Button::new(label).frame(false)).on_hover_cursor(CursorIcon::PointingHand);
        ui.painter().set(background, button_feedback(ui, &response));
        response
    }
}
// The edge owns resizing; the rest of a heading remains available for sorting/reordering.
fn column_resize(ui: &Ui, table: &str, key: &str, rect: Rect, width: f32) -> Option<(f32, bool)> {
    let id = ui.id().with(("column-resize", table, key));
    let edge = Rect::from_center_size(pos2(rect.right(), rect.center().y), vec2(8.0, rect.height()));
    let response = ui.interact(edge, id, Sense::click_and_drag()).on_hover_cursor(CursorIcon::ResizeHorizontal);
    if response.hovered() || response.dragged() {
        ui.painter().vline(rect.right(), rect.y_range(), Stroke::new(2.0, ACCENT));
    }
    if response.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.right_center());
        ui.ctx().data_mut(|data| data.insert_temp(id, (width, origin.x)));
    }
    if response.dragged() || response.drag_stopped() {
        let (start, origin) = ui.ctx().data(|data| data.get_temp::<(f32, f32)>(id)).unwrap_or((width, rect.right()));
        let pointer = ui.input(|i| i.pointer.interact_pos()).map_or(origin, |p| p.x);
        return Some(((start + pointer - origin).clamp(48.0, 2000.0), response.drag_stopped()));
    }
    None
}
fn bg(ctx: &Context) -> Color32 {
    if ctx.style().visuals.dark_mode {
        Color32::from_rgb(24, 24, 24)
    } else {
        Color32::from_rgb(250, 250, 250)
    }
}
fn shell(ctx: &Context) -> Color32 {
    if ctx.style().visuals.dark_mode {
        Color32::from_rgb(31, 33, 34)
    } else {
        Color32::from_rgb(240, 241, 243)
    }
}
fn card(ctx: &Context) -> Color32 {
    if ctx.style().visuals.dark_mode {
        Color32::from_rgb(40, 42, 43)
    } else {
        Color32::WHITE
    }
}
fn line(ctx: &Context) -> Color32 {
    if ctx.style().visuals.dark_mode {
        Color32::from_rgb(49, 49, 49)
    } else {
        Color32::from_gray(220)
    }
}
fn fg(ctx: &Context) -> Color32 {
    if ctx.style().visuals.dark_mode {
        Color32::from_gray(226)
    } else {
        Color32::from_gray(30)
    }
}
fn muted(ctx: &Context) -> Color32 {
    if ctx.style().visuals.dark_mode {
        Color32::from_gray(184)
    } else {
        Color32::from_gray(90)
    }
}
fn text(p: &Painter, pos: Pos2, t: impl Into<String>, size: f32, color: Color32) {
    p.text(pos, Align2::LEFT_CENTER, tr(&t.into()), FontId::proportional(size), color);
}
fn cell(ui: &Ui, rect: Rect, t: impl Into<String>, right: bool, color: Color32, size: f32) {
    let p = ui.painter().with_clip_rect(rect.shrink2(vec2(7.0, 0.0)));
    p.text(
        if right { pos2(rect.right() - 9.0, rect.center().y) } else { pos2(rect.left() + 9.0, rect.center().y) },
        if right { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
        tr(&t.into()),
        FontId::proportional(size),
        color,
    );
}
fn resource_heat(column: Column, process: &Process, mem_total: u64, dark: bool) -> Option<Color32> {
    // Use fixed resource thresholds so idle cells stay neutral even when they
    // happen to be the busiest in a quiet system. Unknown readings have no heat.
    let (value, threshold, high) = match column {
        Column::Cpu => (process.cpu, 5.0, 50.0),
        Column::TotalCpu => (process.total_cpu, 5.0, 50.0),
        Column::Memory | Column::TotalMemory if mem_total > 0 => {
            let memory = if column == Column::Memory { process.memory } else { process.total_memory };
            (memory as f32 / mem_total as f32 * 100.0, 5.0, 25.0)
        }
        Column::Disk if process.io_available => ((process.read_rate + process.write_rate) / 1_048_576.0, 1.0, 10.0),
        _ => return None,
    };
    if !value.is_finite() || value <= threshold {
        return None;
    }
    let strength = ((value - threshold) / (high - threshold)).clamp(0.0, 1.0);
    Some(if dark {
        Color32::from_rgb((62.0 + strength * 78.0) as u8, 23, 22)
    } else {
        Color32::from_rgb(255, (239.0 - strength * 65.0) as u8, (232.0 - strength * 65.0) as u8)
    })
}
fn icon(p: &Painter, c: Pos2, kind: usize, color: Color32) {
    let st = Stroke::new(1.4, color);
    let r = Rect::from_center_size(c, vec2(17.0, 17.0));
    match kind {
        0 => {
            p.rect_stroke(r, 1.0, st, StrokeKind::Inside);
            p.line_segment([pos2(c.x - 2.0, r.top()), pos2(c.x - 2.0, r.bottom())], st);
            p.line_segment([pos2(r.left(), c.y - 2.0), pos2(r.right(), c.y - 2.0)], st);
        }
        1 => {
            p.rect_stroke(r, 2.0, st, StrokeKind::Inside);
            p.add(Shape::line(
                vec![c + vec2(-6.0, 1.0), c + vec2(-3.0, 1.0), c + vec2(-1.0, -5.0), c + vec2(2.0, 5.0), c + vec2(4.0, -1.0), c + vec2(7.0, -1.0)],
                st,
            ));
        }
        2 => {
            p.circle_stroke(c, 8.0, st);
            p.line_segment([c, c + vec2(0.0, -5.0)], st);
            p.line_segment([c, c + vec2(5.0, 2.0)], st);
        }
        3 => {
            p.circle_stroke(c, 7.0, st);
            p.line_segment([c, c + vec2(4.0, -5.0)], st);
            p.rect_filled(Rect::from_min_max(c + vec2(-10.0, 3.0), c + vec2(10.0, 10.0)), 0.0, p.ctx().style().visuals.panel_fill);
        }
        4 => {
            p.circle_stroke(c + vec2(-2.0, -4.0), 4.0, st);
            p.rect_stroke(Rect::from_center_size(c + vec2(-2.0, 5.0), vec2(13.0, 7.0)), 3.0, st, StrokeKind::Inside);
            p.circle_stroke(c + vec2(6.0, -1.0), 3.0, st);
        }
        5 => {
            for y in [-6.0, 0.0, 6.0] {
                p.circle_filled(c + vec2(-8.0, y), 1.0, color);
                p.line_segment([c + vec2(-3.0, y), c + vec2(9.0, y)], st);
            }
        }
        6 | 7 => {
            p.circle_stroke(c, 6.0, st);
            p.circle_stroke(c, 2.0, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let v = vec2(a.cos(), a.sin());
                p.line_segment([c + v * 6.0, c + v * 9.0], st);
            }
        }
        8 => {
            for y in [-5.0, 0.0, 5.0] {
                p.line_segment([c + vec2(-8.0, y), c + vec2(8.0, y)], st);
            }
        }
        9 => {
            p.rect_stroke(r, 1.0, st, StrokeKind::Inside);
            p.line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 5.0)], st);
            p.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], st);
        }
        10 | 11 => {
            for (x, y) in [(-7.0, -3.0), (-1.0, -3.0), (5.0, -3.0), (-1.0, -9.0)] {
                p.rect_stroke(Rect::from_min_size(c + vec2(x, y), vec2(5.0, 5.0)), 0.0, st, StrokeKind::Inside);
            }
            p.line_segment([c + vec2(-9.0, 5.0), c + vec2(9.0, 5.0)], st);
            p.line_segment([c + vec2(-7.0, 5.0), c + vec2(-4.0, 9.0)], st);
            p.line_segment([c + vec2(-4.0, 9.0), c + vec2(5.0, 9.0)], st);
            p.line_segment([c + vec2(5.0, 9.0), c + vec2(9.0, 5.0)], st);
        }
        _ => {
            p.circle_stroke(c, 8.0, st);
            p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], st);
        }
    }
}
fn graph(ui: &Ui, r: Rect, series: &Series, max: f32, color: Color32, grid: bool, fill: bool) {
    let p = ui.painter().with_clip_rect(r);
    if grid {
        for i in 0..=12 {
            let x = r.left() + r.width() * i as f32 / 12.0;
            p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(1.0, line(ui.ctx())));
        }
        for i in 0..=10 {
            let y = r.top() + r.height() * i as f32 / 10.0;
            p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0, line(ui.ctx())));
        }
    }
    if !series.is_empty() {
        let start = HISTORY - series.len();
        let points: Vec<_> = series
            .iter()
            .enumerate()
            .map(|(i, v)| {
                pos2(r.left() + r.width() * (start + i) as f32 / (HISTORY - 1) as f32, r.bottom() - r.height() * (v / max.max(0.01)).clamp(0.0, 1.0))
            })
            .collect();
        if fill && points.len() > 1 {
            let mut mesh = egui::Mesh::default();
            let color = color.gamma_multiply(0.16);
            for w in points.windows(2) {
                let i = mesh.vertices.len() as u32;
                for pos in [pos2(w[0].x, r.bottom()), w[0], w[1], pos2(w[1].x, r.bottom())] {
                    mesh.colored_vertex(pos, color);
                }
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i, i + 2, i + 3);
            }
            p.add(Shape::mesh(mesh));
        }
        if points.len() > 1 {
            p.add(PathShape::line(points, Stroke::new(1.4, color)));
        }
    }
    p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_gray(100)), StrokeKind::Inside);
}
fn metric(ui: &mut Ui, label: &str, value: impl Into<String>) {
    ui.vertical(|ui| {
        ui.label(RichText::new(tr(label)).size(13.0).color(muted(ui.ctx())));
        ui.label(RichText::new(tr(&value.into())).size(24.0));
    });
}
impl TaskManager {
    fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, page: Option<Page>, resource: Resource) -> Self {
        let mut settings = Settings::load();
        if std::env::args().any(|a| a == "--low-memory") {
            settings.low_memory = true;
        }
        if screenshot.is_some() {
            settings.theme = "Dark".into();
            settings.always_top = false;
            settings.low_memory = std::env::args().any(|a| a == "--low-memory");
            settings.interval = 1000;
        }
        if let Some(value) = std::env::args().skip_while(|v| v != "--language").nth(1) {
            settings.language = Language::from(&value);
        }
        i18n::set_language(settings.language);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = if settings.theme == "Light" { Visuals::light() } else { Visuals::dark() };
        style.visuals.override_text_color = Some(if settings.theme == "Light" { Color32::from_gray(30) } else { Color32::from_gray(226) });
        style.visuals.selection.bg_fill = Color32::from_rgb(66, 50, 46);
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(75, 70, 68);
        style.visuals.window_corner_radius = CornerRadius::same(9);
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(12.0, 8.0);
        style.text_styles.insert(TextStyle::Body, FontId::proportional(15.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        style.visuals.panel_fill = bg(&cc.egui_ctx);
        cc.egui_ctx.set_style(style);
        let monitor = if demo::enabled() { Monitor::demo() } else { Monitor::start(cc.egui_ctx.clone()) };
        monitor.interval.store(if settings.low_memory { 5000 } else { settings.interval }, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        Self {
            docker: docker_page::DockerView::new(&cc.egui_ctx),
            application: None,
            properties: None,
            metadata_busy: false,
            network_samples: HashMap::new(),
            application_request: std::env::args().skip_while(|a| a != "--application").nth(1).and_then(|a| a.parse().ok()),
            monitor,
            data: Arc::new(Snapshot::default()),
            page: page.unwrap_or(settings.start),
            resource,
            collapsed: settings.low_memory,
            search: String::new(),
            sort: Sort::Memory,
            descending: true,
            selected: None,
            kill_dialog: None,
            force: false,
            run_dialog: false,
            command: String::new(),
            message: None,
            settings,
            services: vec![],
            services_error: String::new(),
            service_selected: None,
            startups: vec![],
            startup_selected: None,
            busy: false,
            tx,
            rx,
            last_services: Instant::now() - Duration::from_secs(60),
            history: HashMap::new(),
            session: Instant::now(),
            logical: true,
            screenshot,
            frames: 0,
            screenshot_requested: false,
            folded: HashSet::new(),
            expanded_apps: HashSet::new(),
            authorization: None,
            app_icons: HashMap::new(),
        }
    }
    fn notify(&mut self, message: impl Into<String>, error: bool) {
        let message = message.into();
        self.message = Some((tr(&message).into(), error, Instant::now()));
    }
    fn switch(&mut self, page: Page) {
        self.application = None;
        self.properties = None;
        if self.page != page {
            self.selected = None;
        }
        self.page = page;
        self.docker.back();
        self.search.clear();
        if page == Page::Services {
            self.load_services();
        }
        if page == Page::Startup {
            self.load_startups();
        }
    }
    fn load_services(&mut self) {
        if demo::enabled() {
            self.services = demo::services();
            self.last_services = Instant::now();
            return;
        }
        if self.busy {
            return;
        }
        self.busy = true;
        self.last_services = Instant::now();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (s, e) = services();
            let _ = tx.send(Worker::Services(s, e));
        });
    }
    fn load_startups(&mut self) {
        if demo::enabled() {
            self.startups = demo::startups();
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Worker::Startups(startups()));
        });
    }
    fn selected_process(&self) -> Option<Process> {
        let (id, start) = self.selected?;
        self.data.processes.iter().find(|p| p.pid == id && p.start == start).cloned()
    }
    fn process_action(&mut self, p: &Process, action: ProcessAction) {
        if demo::enabled() {
            self.notify("Demo mode: system actions are disabled.", false);
            return;
        }
        let action = PrivilegedAction::Process { pid: p.pid, start: p.start, name: p.name.clone(), action };
        match action.run() {
            Ok(()) => self.notify("Action completed.", false),
            Err(e) if permission_denied(&e) && matches!(&action, PrivilegedAction::Process { action: ProcessAction::Efficiency, .. }) => {
                self.notify("This process priority change requires administrator access and cannot be safely authorized.", true)
            }
            Err(e) if permission_denied(&e) && !matches!(&action, PrivilegedAction::Process { action: ProcessAction::Efficiency, .. }) => {
                self.authorization = Some(action)
            }
            Err(e) => self.notify(e, true),
        }
    }
    fn ask_end(&mut self) {
        if let Some(p) = self.selected_process() {
            if p.pid <= 1 || p.pid == std::process::id() {
                self.notify("This process is protected.", true);
            } else {
                self.force = false;
                self.kill_dialog = Some(p);
            }
        }
    }
    fn update_data(&mut self) {
        let fresh = self.monitor.latest.lock().ok().and_then(|mut l| l.take());
        if let Some(data) = fresh {
            if data.sampled != self.data.sampled {
                let now = Instant::now();
                let clock = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
                let dt = if self.data.sampled > 0 { self.monitor.interval.load(Ordering::Relaxed) as f64 / 1000.0 } else { 0.0 };
                for p in data.processes.iter().filter(|_| !self.settings.low_memory) {
                    let h = self.history.entry((p.pid, p.start)).or_insert_with(|| HistoryRow {
                        name: p.name.clone(),
                        pid: p.pid,
                        cpu_seconds: 0.0,
                        read: 0.0,
                        write: 0.0,
                        last_ticks: p.ticks,
                        last_seen: now,
                    });
                    h.cpu_seconds += p.ticks.saturating_sub(h.last_ticks) as f64 / clock;
                    h.read += p.read_rate as f64 * dt;
                    h.write += p.write_rate as f64 * dt;
                    h.last_ticks = p.ticks;
                    h.last_seen = now;
                }
                if self.history.len() > 4096 {
                    self.history.retain(|_, h| now.duration_since(h.last_seen) < Duration::from_secs(60));
                }
                if !self.folded.is_empty() || !self.expanded_apps.is_empty() {
                    let live: HashSet<_> = data.processes.iter().map(|p| (p.pid, p.start)).collect();
                    self.folded.retain(|key| live.contains(key));
                    self.expanded_apps.retain(|key| live.contains(key));
                }
                if self.resource == Resource::Gpu(usize::MAX) {
                    self.resource = Resource::Gpu(
                        data.gpus.iter().position(|g| g.busy.is_some()).or_else(|| data.gpus.iter().position(|g| g.nvidia)).unwrap_or(0),
                    );
                }
                if let Some(application) = &mut self.application {
                    application.refresh(&data, self.settings.low_memory);
                }
                self.data = data;
            }
        }
        self.network_samples
            .retain(|key, (_, at)| at.elapsed() < Duration::from_secs(10) && self.data.processes.iter().any(|p| (p.pid, p.start) == *key));
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Worker::UpdateCheck(key, result) => {
                    if let Some(view) = self.application.as_mut().filter(|a| a.key() == key) {
                        view.update = match result {
                            Ok(Some(available)) => application_page::UpdateState::Available(available),
                            Ok(None) => application_page::UpdateState::Current,
                            Err(e) => application_page::UpdateState::Error(e),
                        };
                    }
                }
                Worker::Updated(key, result) => {
                    self.busy = false;
                    if let Some(view) = self.application.as_mut().filter(|a| a.key() == key) {
                        view.update = match &result {
                            Ok(()) => {
                                view.metadata = None;
                                application_page::UpdateState::Idle
                            }
                            Err(e) => application_page::UpdateState::Error(e.clone()),
                        };
                    }
                    match result {
                        Ok(()) => self.notify("Update completed. Restart the application to use the installed version.", false),
                        Err(e) => self.notify(e, true),
                    }
                }
                Worker::Network(key, result) => {
                    self.busy = false;
                    match result {
                        Ok(rate) if rate.is_finite() && rate >= 0.0 && self.data.processes.iter().any(|p| (p.pid, p.start) == key) => {
                            self.network_samples.insert(key, (rate, Instant::now()));
                            self.notify("Network sample captured. The reading is shown for 10 seconds; click again to measure.", false);
                        }
                        Ok(_) => self.notify("The process exited or changed. Select it again.", true),
                        Err(e) => self.notify(e, true),
                    }
                }
                Worker::Application(key, metadata) => {
                    self.metadata_busy = false;
                    if let Some(application) = self.application.as_mut().filter(|a| a.key() == key) {
                        application.metadata = Some(metadata);
                    }
                }
                Worker::Services(s, e) => {
                    let selected = self.service_selected.and_then(|i| self.services.get(i)).map(|s| (s.name.clone(), s.scope.clone()));
                    self.services = s;
                    self.service_selected = selected.and_then(|(name, scope)| self.services.iter().position(|s| s.name == name && s.scope == scope));
                    self.services_error = e;
                    self.busy = false;
                    if self.service_selected.is_some_and(|i| i >= self.services.len()) {
                        self.service_selected = None;
                    }
                }
                Worker::Startups(s) => self.startups = s,
                Worker::Authorize(action) => {
                    self.busy = false;
                    self.authorization = Some(action);
                }
                Worker::Action(r) => {
                    self.busy = false;
                    match r {
                        Ok(()) => self.notify("Action completed.", false),
                        Err(e) => self.notify(e, true),
                    }
                    if self.page == Page::Services {
                        self.load_services();
                    }
                    if self.page == Page::Startup {
                        self.load_startups();
                    }
                }
            }
        }
        if self.page == Page::Services && self.last_services.elapsed() > Duration::from_secs(10) && !self.busy {
            self.load_services();
        }
    }
    fn titlebar(&mut self, ctx: &Context) {
        TopBottomPanel::top("titlebar").exact_height(60.0).frame(Frame::NONE.fill(shell(ctx))).show(ctx, |ui| {
            let r = ui.max_rect();
            let drag = ui.interact(r, ui.id().with("drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            }
            let logo = Rect::from_min_size(r.min + vec2(24.0, 22.0), vec2(20.0, 17.0));
            ui.painter().rect_filled(logo, 1.0, Color32::from_rgb(26, 100, 139));
            ui.painter().add(Shape::line(
                vec![
                    logo.min + vec2(2.0, 12.0),
                    logo.min + vec2(6.0, 6.0),
                    logo.min + vec2(10.0, 13.0),
                    logo.min + vec2(14.0, 4.0),
                    logo.min + vec2(18.0, 11.0),
                ],
                Stroke::new(1.8, Color32::from_rgb(145, 219, 241)),
            ));
            text(ui.painter(), pos2(r.left() + 62.0, r.center().y), "Task Manager", 14.0, fg(ctx));
            if self.page != Page::Performance && self.page != Page::Settings {
                let width = (r.width() * 0.36).clamp(250.0, 490.0);
                let sr = Rect::from_center_size(pos2(r.center().x, r.center().y), vec2(width, 36.0));
                ui.scope_builder(UiBuilder::new().max_rect(sr), |ui| {
                    Frame::NONE.fill(card(ctx)).corner_radius(6.0).inner_margin(Margin::symmetric(12, 5)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (ir, _) = ui.allocate_exact_size(vec2(20.0, 24.0), Sense::hover());
                            let c = ir.center() - vec2(2.0, 2.0);
                            ui.painter().circle_stroke(c, 5.0, Stroke::new(1.5, muted(ctx)));
                            ui.painter().line_segment([c + vec2(4.0, 4.0), c + vec2(8.0, 8.0)], Stroke::new(1.5, muted(ctx)));
                            let response = ui.add_sized(
                                vec2(width - 53.0, 24.0),
                                TextEdit::singleline(&mut self.search)
                                    .frame(false)
                                    .hint_text(tr(if self.page == Page::Docker {
                                        "Search projects, containers or images"
                                    } else {
                                        "Type a name, user, or PID to search"
                                    }))
                                    .id(Id::new("search")),
                            );
                            if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::F)) {
                                response.request_focus();
                            }
                        });
                    });
                });
            }
            for (i, label) in ["—", "□", "×"].iter().enumerate() {
                let br = Rect::from_min_size(pos2(r.right() - 138.0 + i as f32 * 46.0, r.top()), vec2(46.0, 44.0));
                let resp = ui.interact(br, ui.id().with(("window", i)), Sense::click());
                ui.painter().add(button_feedback(ui, &resp));
                if i == 2 && resp.hovered() {
                    ui.painter().rect_filled(br, 0.0, if i == 2 { Color32::from_rgb(175, 45, 45) } else { card(ctx) });
                }
                ui.painter().text(br.center(), Align2::CENTER_CENTER, label, FontId::proportional(if i == 2 { 23.0 } else { 18.0 }), fg(ctx));
                if resp.clicked() {
                    match i {
                        0 => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                        1 => ctx.send_viewport_cmd(ViewportCommand::Maximized(!ctx.input(|i| i.viewport().maximized.unwrap_or(false)))),
                        _ => ctx.send_viewport_cmd(ViewportCommand::Close),
                    }
                }
            }
        });
    }
    fn sidebar(&mut self, ctx: &Context) {
        let width = if self.collapsed || ctx.screen_rect().width() < 1100.0 { 64.0 } else { 274.0 };
        SidePanel::left("navigation")
            .exact_width(width)
            .resizable(false)
            .frame(Frame::NONE.fill(shell(ctx)).inner_margin(Margin::symmetric(6, 10)))
            .show(ctx, |ui| {
                let r = ui.allocate_rect(Rect::from_min_size(ui.cursor().min, vec2(width - 12.0, 38.0)), Sense::click());
                ui.painter().add(button_feedback(ui, &r));
                r.clone().on_hover_cursor(CursorIcon::PointingHand);
                icon(ui.painter(), r.rect.min + vec2(24.0, 19.0), 8, muted(ctx));
                if r.clicked() {
                    self.collapsed = !self.collapsed;
                }
                ui.add_space(4.0);
                for page in Page::all().into_iter().filter(|p| *p != Page::Settings) {
                    if page != Page::Docker || self.docker.installed {
                        self.nav_item(ui, page, width);
                    }
                }
                let remaining = ui.available_height() - 48.0;
                ui.add_space(remaining.max(0.0));
                self.nav_item(ui, Page::Settings, width);
            });
    }
    fn nav_item(&mut self, ui: &mut Ui, page: Page, width: f32) {
        let (r, resp) = ui.allocate_exact_size(vec2(width - 12.0, 44.0), Sense::click());
        let active = self.page == page;
        if active || resp.hovered() {
            ui.painter().rect_filled(r, 5.0, if active { card(ui.ctx()) } else { card(ui.ctx()).gamma_multiply(0.8) });
        }
        if active {
            ui.painter().rect_filled(Rect::from_min_size(pos2(r.left(), r.center().y - 9.0), vec2(3.0, 18.0)), 2.0, ACCENT);
        }
        ui.painter().add(button_feedback(ui, &resp));
        resp.clone().on_hover_cursor(CursorIcon::PointingHand);
        icon(ui.painter(), pos2(r.left() + 24.0, r.center().y), page.icon(), fg(ui.ctx()));
        if !self.collapsed && width > 100.0 {
            text(ui.painter(), pos2(r.left() + 52.0, r.center().y), page.title(), 15.0, fg(ui.ctx()));
        }
        if resp.on_hover_text(tr(page.title())).clicked() {
            self.switch(page);
        }
    }
    fn toolbar(&mut self, ui: &mut Ui) {
        if self.page == Page::Docker {
            self.docker.toolbar(ui, &mut self.settings);
            return;
        }
        if self.application.is_some() {
            self.application_toolbar(ui);
            return;
        }
        let ctx = ui.ctx().clone();
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
        text(ui.painter(), pos2(r.left() + 20.0, r.center().y), self.page.title(), 17.0, fg(&ctx));
        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, line(&ctx)));
        let mut toolbar = ui
            .new_child(UiBuilder::new().id_salt("toolbar-actions").max_rect(r.shrink2(vec2(15.0, 9.0))).layout(Layout::right_to_left(Align::Center)));
        let ui = &mut toolbar;
        {
            ui.menu_button(RichText::new(tr("•••")).size(18.0), |ui| {
                if ui.button(tr(if self.settings.interval == 0 { "Resume updates" } else { "Pause updates" })).clicked() {
                    self.settings.interval = if self.settings.interval == 0 { 1000 } else { 0 };
                    self.apply_settings(&ctx);
                    ui.close_menu();
                }
                if ui.button(tr("Settings")).clicked() {
                    self.switch(Page::Settings);
                    ui.close_menu();
                }
                ui.label(tr("Refresh only reads Linux system data."));
            });
            match self.page {
                Page::Apps | Page::Processes | Page::Details => {
                    ui.menu_button(tr("Columns"), |ui| self.column_menu(ui, self.page == Page::Details));
                    if self.page != Page::Details {
                        let mut changed = false;
                        ui.menu_button(tr(if self.settings.tree { "Tree view" } else { "List view" }), |ui| {
                            changed |= ui.selectable_value(&mut self.settings.tree, true, tr("Tree view")).changed();
                            changed |= ui.selectable_value(&mut self.settings.tree, false, tr("List view")).changed();
                            ui.separator();
                            if ui.button(tr("Expand all")).clicked() {
                                if self.page == Page::Apps {
                                    self.expanded_apps = self.data.processes.iter().map(|p| (p.pid, p.start)).collect();
                                } else {
                                    self.folded.clear();
                                }
                                ui.close_menu();
                            }
                            if ui.button(tr("Collapse all")).clicked() {
                                if self.page == Page::Apps {
                                    self.expanded_apps.clear();
                                } else {
                                    self.folded = self.data.processes.iter().map(|p| (p.pid, p.start)).collect();
                                }
                                ui.close_menu();
                            }
                        });
                        if changed {
                            self.apply_settings(&ctx);
                        }
                    }
                    if ui.add_enabled(self.selected_process().is_some() && !demo::enabled(), quiet_button(tr("End task"))).clicked() {
                        self.ask_end();
                    }
                    if ui
                        .add_enabled(self.selected_process().is_some() && !demo::enabled(), quiet_button(tr("Efficiency mode")))
                        .on_hover_text(tr("Reduce scheduling priority (nice +10)"))
                        .clicked()
                    {
                        if let Some(p) = self.selected_process() {
                            self.process_action(&p, ProcessAction::Efficiency);
                        }
                    }
                }
                Page::Services => {
                    for (label, action) in [("↻  Restart", "restart"), ("□  Stop", "stop"), ("▷  Start", "start")] {
                        if ui.add_enabled(self.service_selected.is_some() && !self.busy && !demo::enabled(), quiet_button(tr(label))).clicked() {
                            if let Some(s) = self.service_selected.and_then(|i| self.services.get(i)).cloned() {
                                self.busy = true;
                                let tx = self.tx.clone();
                                std::thread::spawn(move || {
                                    let operation = PrivilegedAction::Service { service: s, action: action.into() };
                                    let result = operation.run();
                                    let response = if result.as_ref().err().is_some_and(|e| permission_denied(e)) {
                                        Worker::Authorize(operation)
                                    } else {
                                        Worker::Action(result)
                                    };
                                    let _ = tx.send(response);
                                });
                            }
                        }
                    }
                }
                Page::Startup => {
                    if ui.add_enabled(self.startup_selected.is_some() && !demo::enabled(), quiet_button(tr("Enable / disable"))).clicked() {
                        if let Some(s) = self.startup_selected.and_then(|i| self.startups.get(i)).cloned() {
                            let tx = self.tx.clone();
                            std::thread::spawn(move || {
                                let _ = tx.send(Worker::Action(toggle_startup(&s)));
                            });
                        }
                    }
                }
                Page::History => {
                    if ui.button(tr("Clear history")).clicked() {
                        self.history.clear();
                        self.session = Instant::now();
                    }
                }
                _ => {}
            }
            if self.page != Page::Settings && ui.add_enabled(!demo::enabled(), quiet_button(tr("⊞  Run new task"))).clicked() {
                self.run_dialog = true;
            }
        }
    }
    fn statusbar(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status").exact_height(30.0).frame(Frame::NONE.fill(bg(ctx))).show(ctx, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(14.0);
                let t = if demo::enabled() {
                    "Demo · fictional data".into()
                } else if self.settings.interval == 0 {
                    "Updates paused".into()
                } else if self.settings.low_memory {
                    "Low memory mode · 5 s".into()
                } else {
                    format!("{} · {:.1} s", tr("Live"), self.settings.interval as f32 / 1000.0)
                };
                ui.label(RichText::new(tr(&t)).size(12.0).color(muted(ctx)));
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ScrollArea::horizontal().id_salt("footer-metrics").scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden).show(
                        ui,
                        |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add_space(14.0);
                                for (index, item) in FooterItem::all().into_iter().filter(|item| self.settings.footer.contains(item)).enumerate() {
                                    if index > 0 {
                                        ui.separator();
                                    }
                                    ui.label(RichText::new(item.value(&self.data)).size(12.0).color(muted(ctx)))
                                        .on_hover_text(tr(item.description()));
                                }
                            });
                        },
                    );
                });
            });
        });
    }
    fn column_menu(&mut self, ui: &mut Ui, details: bool) {
        ui.label(RichText::new(tr("Select columns")).strong());
        ui.separator();
        let columns = if details { &mut self.settings.detail_columns } else { &mut self.settings.columns };
        let mut changed = false;
        ui.label(RichText::new(tr("Drag column headings to reorder; drag their edges to resize.")).small());
        ScrollArea::vertical().max_height(540.0).show(ui, |ui| {
            let mut menu_order = columns.clone();
            menu_order.extend(Column::all().into_iter().filter(|c| !columns.contains(c)));
            for column in menu_order {
                ui.horizontal(|ui| {
                    let mut visible = columns.contains(&column);
                    let response = ui.add_enabled(column != Column::Name, Checkbox::new(&mut visible, tr(column.key())));
                    if response.changed() {
                        columns.retain(|c| *c != column);
                        if visible {
                            columns.push(column);
                        }
                        changed = true;
                    }
                    if let Some(index) = columns.iter().position(|c| *c == column) {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(index + 1 < columns.len(), Button::new(">")).on_hover_text(tr("Move right")).clicked() {
                                columns.swap(index, index + 1);
                                changed = true;
                            }
                            if ui.add_enabled(index > 0, Button::new("<")).on_hover_text(tr("Move left")).clicked() {
                                columns.swap(index, index - 1);
                                changed = true;
                            }
                        });
                    }
                });
            }
        });
        ui.separator();
        if ui.button(tr("Reset columns")).clicked() {
            *columns = if details { Column::details() } else { Column::defaults() };
            let prefix = if details { "details/" } else { "process/" };
            self.settings.column_widths.retain(|key, _| !key.starts_with(prefix));
            changed = true;
        }
        ui.label(RichText::new(tr("Unavailable counters are shown as —.")).small());
        if changed {
            self.apply_settings(ui.ctx());
        }
    }
    fn process_table(&mut self, ui: &mut Ui, details: bool) {
        let data = self.data.clone();
        let query = self.search.to_lowercase();
        let matching: HashSet<u32> = data
            .processes
            .iter()
            .filter(|p| {
                query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || p.user.to_lowercase().contains(&query)
                    || p.pid.to_string().contains(&query)
                    || p.command.to_lowercase().contains(&query)
            })
            .map(|p| p.pid)
            .collect();
        let mut order: Vec<usize> = (0..data.processes.len()).collect();
        order.sort_unstable_by(|&a, &b| {
            let (a, b) = (&data.processes[a], &data.processes[b]);
            let c = match self.sort {
                Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Sort::Cpu => a.cpu.total_cmp(&b.cpu),
                Sort::TotalCpu => a.total_cpu.total_cmp(&b.total_cpu),
                Sort::Memory => a.memory.cmp(&b.memory),
                Sort::TotalMemory => a.total_memory.cmp(&b.total_memory),
                Sort::Disk => (a.read_rate + a.write_rate).total_cmp(&(b.read_rate + b.write_rate)),
                Sort::Pid => a.pid.cmp(&b.pid),
                Sort::Parent => a.parent.cmp(&b.parent),
                Sort::User => a.user.cmp(&b.user),
                Sort::Command => a.command.cmp(&b.command),
                Sort::Threads => a.threads.cmp(&b.threads),
                Sort::Priority => a.nice.cmp(&b.nice),
                Sort::Read => a.read_rate.total_cmp(&b.read_rate),
                Sort::Write => a.write_rate.total_cmp(&b.write_rate),
                Sort::Status => a.status.cmp(&b.status),
            }
            .then_with(|| a.pid.cmp(&b.pid));
            if self.descending {
                c.reverse()
            } else {
                c
            }
        });
        let tree = self.settings.tree && !details;
        let view = if details {
            process_view::View::Details
        } else if self.page == Page::Apps {
            process_view::View::Apps
        } else {
            process_view::View::Background
        };
        // Applications start folded, including new arrivals after a refresh.
        // Background processes start expanded and keep independent user choices.
        let app_folds;
        let folded = if view == process_view::View::Apps {
            app_folds = data.processes.iter().map(|p| (p.pid, p.start)).filter(|key| !self.expanded_apps.contains(key)).collect();
            &app_folds
        } else {
            &self.folded
        };
        let rows = process_view::display_rows(&data.processes, &order, &matching, folded, !query.is_empty(), tree, view);
        ui.spacing_mut().item_spacing.y = 0.0;
        let columns = if details { self.settings.detail_columns.clone() } else { self.settings.columns.clone() };
        let table = if details { "details" } else { "process" };
        let mut widths: Vec<f32> = columns.iter().map(|c| self.column_width(table, c.key(), c.width())).collect();
        let extra = (ui.available_width() - widths.iter().sum::<f32>()).max(0.0);
        if !self.settings.column_widths.contains_key(&format!("{table}/Name")) {
            if let Some(i) = columns.iter().position(|c| *c == Column::Name) {
                widths[i] += extra;
            }
        }
        let width = widths.iter().sum::<f32>();
        let mut resize = None;
        let mut reorder = None;
        ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
        ScrollArea::horizontal()
            .id_salt(("process-h", view))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(width);
                let (head, _) = ui.allocate_exact_size(vec2(width, if details { 34.0 } else { 61.0 }), Sense::hover());
                let mut x = head.left();
                for (i, &column) in columns.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(x, head.top()), vec2(widths[i], head.height()));
                    ui.painter().vline(x, r.y_range(), Stroke::new(1.0, line(ui.ctx())));
                    let selected = column.sort().is_some_and(|s| s == self.sort);
                    if selected {
                        ui.painter().rect_filled(r, 0.0, card(ui.ctx()));
                    }
                    if !details {
                        let total = match column {
                            Column::Cpu | Column::TotalCpu => format!("{:.0}%", data.cpu),
                            Column::Memory | Column::TotalMemory => format!("{:.0}%", data.mem_percent()),
                            Column::Disk => format!("{:.0}%", data.disks.iter().map(|d| d.busy).fold(0.0, f32::max)),
                            Column::Gpu => data.gpus.iter().filter_map(|g| g.busy).reduce(f32::max).map(|v| format!("{v:.0}%")).unwrap_or_default(),
                            _ => String::new(),
                        };
                        cell(ui, Rect::from_min_size(r.min, vec2(r.width(), 30.0)), total, true, fg(ui.ctx()), 20.0);
                    }
                    if selected {
                        let center = pos2(if column.numeric() { r.left() + 13.0 } else { r.center().x }, r.top() + 8.0);
                        let offsets = if self.descending {
                            [vec2(-3.0, -1.5), vec2(0.0, 1.5), vec2(3.0, -1.5)]
                        } else {
                            [vec2(-3.0, 1.5), vec2(0.0, -1.5), vec2(3.0, 1.5)]
                        };
                        ui.painter().add(Shape::line(offsets.iter().map(|d| center + *d).collect(), Stroke::new(1.2, muted(ui.ctx()))));
                    }
                    cell(
                        ui,
                        if details { r } else { Rect::from_min_max(pos2(r.left(), r.bottom() - 28.0), r.max) },
                        tr(column.key()),
                        column.numeric(),
                        muted(ui.ctx()),
                        14.0,
                    );
                    let body = Rect::from_min_max(r.min + vec2(4.0, 0.0), r.max - vec2(4.0, 0.0));
                    let resp = ui.interact(body, ui.id().with(("sort", column)), Sense::click_and_drag());
                    if let Some(change) = column_resize(ui, table, column.key(), r, widths[i]) {
                        resize = Some((column.key(), change));
                    }
                    resp.dnd_set_drag_payload(column);
                    if let Some(source) = resp.dnd_hover_payload::<Column>() {
                        if *source != column {
                            ui.painter().vline(r.left(), r.y_range(), Stroke::new(3.0, ACCENT));
                        }
                    }
                    if let Some(source) = resp.dnd_release_payload::<Column>() {
                        if *source != column {
                            reorder = Some((*source, column));
                        }
                    }
                    resp.clone().on_hover_text(tr(if column == Column::TotalCpu {
                        "CPU of this process and all descendants, including hidden ones, as a percentage of total system capacity. Click to sort; right-click for columns."
                    } else if column == Column::TotalMemory {
                        "RSS of this process and all descendants, including hidden ones. Shared pages can be counted more than once. Click to sort; right-click for columns."
                    } else {
                        "Drag edges to resize · Drag to move · Click to sort · Right-click for columns"
                    }));
                    if resp.clicked() {
                        if let Some(sort) = column.sort() {
                            self.descending = if self.sort == sort { !self.descending } else { sort != Sort::Name };
                            self.sort = sort;
                        }
                    }
                    resp.context_menu(|ui| self.column_menu(ui, details));
                    x += widths[i];
                }
                ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, line(ui.ctx())));
                if rows.is_empty() {
                    ui.add_space(48.0);
                    ui.heading(tr("No matching processes"));
                    ui.label(tr("Try another name, user or process ID."));
                    return;
                }
                let row_h = if details { 29.0 } else { 33.0 };
                ScrollArea::vertical().id_salt(("process-v", view)).auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
                    for index in range {
                        let (pi, depth, has_children) = match rows[index] {
                            process_view::DisplayRow::Section(apps, count) => {
                                let (r, _) = ui.allocate_exact_size(vec2(width, row_h), Sense::hover());
                                cell(
                                    ui,
                                    r,
                                    format!("{} ({count})", tr(if apps { "Apps" } else { "Background processes" })),
                                    false,
                                    fg(ui.ctx()),
                                    17.0,
                                );
                                continue;
                            }
                            process_view::DisplayRow::Process(pi, depth, children) => (pi, depth, children),
                        };
                        let p = &data.processes[pi];
                        let key = (p.pid, p.start);
                        let (r, resp) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
                        let selected = self.selected == Some(key);
                        if selected || resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, if selected { card(ui.ctx()).gamma_multiply(1.35) } else { card(ui.ctx()) });
                        }
                        if selected {
                            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 1.0, ACCENT);
                        }
                        let mut x = r.left();
                        let mut toggled = false;
                        for (i, &column) in columns.iter().enumerate() {
                            let cr = Rect::from_min_size(pos2(x, r.top()), vec2(widths[i], row_h));
                            if !details && !selected {
                                if let Some(heat) = resource_heat(column, p, data.mem_total, ui.visuals().dark_mode) {
                                    ui.painter().rect_filled(cr, 0.0, heat);
                                }
                            }
                            if column == Column::Name {
                                let indent = if tree { depth.min(12) as f32 * 18.0 } else { 0.0 };
                                let left = x + 8.0 + indent;
                                let name_painter = ui.painter().with_clip_rect(cr.intersect(ui.clip_rect()));
                                if tree && has_children {
                                    let ar = Rect::from_min_size(pos2(left, r.top()), vec2(19.0, row_h));
                                    let arrow = ui.interact(ar, ui.id().with(("fold", key)), Sense::click());
                                    ui.painter().add(button_feedback(ui, &arrow));
                                    arrow.clone().on_hover_cursor(CursorIcon::PointingHand);
                                    let center = ar.center();
                                    let is_folded = if view == process_view::View::Apps {
                                        !self.expanded_apps.contains(&key)
                                    } else {
                                        self.folded.contains(&key)
                                    };
                                    let offsets = if is_folded && query.is_empty() {
                                        [vec2(-2.0, -4.0), vec2(2.0, 0.0), vec2(-2.0, 4.0)]
                                    } else {
                                        [vec2(-4.0, -2.0), vec2(0.0, 2.0), vec2(4.0, -2.0)]
                                    };
                                    name_painter.add(Shape::line(offsets.iter().map(|d| center + *d).collect(), Stroke::new(1.5, muted(ui.ctx()))));
                                    if arrow.clicked() {
                                        let state = if view == process_view::View::Apps { &mut self.expanded_apps } else { &mut self.folded };
                                        if !state.remove(&key) {
                                            state.insert(key);
                                        }
                                        toggled = true;
                                    }
                                }
                                let c = pos2(left + 29.0, r.center().y);
                                let color = if p.app { BLUE } else { Color32::from_rgb(111, 145, 162) };
                                let ir = Rect::from_center_size(c, vec2(16.0, 16.0));
                                if let Some(app) = &p.desktop {
                                    if let Some(image) = &app.icon {
                                        let texture = self
                                            .app_icons
                                            .entry(app.id.clone())
                                            .or_insert_with(|| ui.ctx().load_texture(&app.id, image.as_ref().clone(), TextureOptions::LINEAR));
                                        name_painter.image(texture.id(), ir, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                                    } else {
                                        icon(&name_painter, c, 0, color);
                                    }
                                } else if p.pid == std::process::id() {
                                    icon(&name_painter, c, 1, CYAN);
                                } else {
                                    name_painter.rect_filled(ir, 2.0, color.gamma_multiply(0.7));
                                    name_painter.rect_stroke(ir.shrink(2.0), 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
                                }
                                cell(ui, Rect::from_min_max(pos2(left + 40.0, r.top()), cr.max), &p.name, false, fg(ui.ctx()), 14.0);
                            } else {
                                let value = match column {
                                    Column::Type => tr(if p.app {
                                        "App"
                                    } else if p.uid == 0 {
                                        "System process"
                                    } else {
                                        "Background process"
                                    })
                                    .into(),
                                    Column::Status => tr(if details || p.status == "Suspended" {
                                        &p.status
                                    } else if p.nice >= 10 {
                                        "Efficiency"
                                    } else {
                                        ""
                                    })
                                    .into(),
                                    Column::Cpu => format!("{:.1}%", p.cpu),
                                    Column::TotalCpu => format!("{:.1}%", p.total_cpu),
                                    Column::Memory => bytes(p.memory),
                                    Column::TotalMemory => bytes(p.total_memory),
                                    Column::Disk => {
                                        if p.io_available {
                                            rate(p.read_rate + p.write_rate)
                                        } else {
                                            "—".into()
                                        }
                                    }
                                    Column::Read => {
                                        if p.io_available {
                                            rate(p.read_rate)
                                        } else {
                                            "—".into()
                                        }
                                    }
                                    Column::Write => {
                                        if p.io_available {
                                            rate(p.write_rate)
                                        } else {
                                            "—".into()
                                        }
                                    }
                                    Column::Network => self.network_samples.get(&key).map(|(value, _)| format!("{} *", rate(*value))).unwrap_or_else(|| "—".into()),
                                    Column::Pid => p.pid.to_string(),
                                    Column::Parent => p.parent.to_string(),
                                    Column::User => p.user.clone(),
                                    Column::Command => p.command.clone(),
                                    Column::Threads => p.threads.to_string(),
                                    Column::Priority => format!("Nice {}", p.nice),
                                    _ => "—".into(),
                                };
                                cell(
                                    ui,
                                    cr,
                                    value,
                                    column.numeric(),
                                    if column == Column::Status && p.nice >= 10 { GREEN } else { fg(ui.ctx()) },
                                    14.0,
                                );
                                if column == Column::Network {
                                    let response = ui.interact(cr, ui.id().with(("network-read", key)), Sense::click());
                                    let response = response.on_hover_cursor(CursorIcon::PointingHand)
                                        .on_hover_text(tr("Click to authenticate and measure this process with NetHogs. * = last authorized sample, shown for 10 seconds."));
                                    if response.clicked() && !demo::enabled() && !self.busy {
                                        self.authorization = Some(process_network::request(p));
                                        toggled = true;
                                    }
                                }
                                if matches!(column, Column::Gpu | Column::GpuEngine | Column::Power)
                                    || (!p.io_available && matches!(column, Column::Disk | Column::Read | Column::Write))
                                {
                                    ui.interact(cr, ui.id().with(("unavailable", key, i)), Sense::hover())
                                        .on_hover_text(tr("This per-process counter is not available from the current Linux driver or permissions."));
                                }
                            }
                            if i > 0 {
                                ui.painter().vline(x, r.y_range(), Stroke::new(1.0, line(ui.ctx())));
                            }
                            x += widths[i];
                        }
                        if resp.double_clicked() && !toggled { self.open_application(p.clone()); }
                        if resp.clicked() && !toggled {
                            self.selected = Some(key);
                        }
                        resp.clone().on_hover_text(format!(
                            "{}\nPID {} · {} {} · {}\n{}",
                            p.name,
                            p.pid,
                            tr("Parent PID"),
                            p.parent,
                            p.user,
                            p.command
                        ));
                        resp.context_menu(|ui| {
                            ui.label(RichText::new(format!("{} · {}", p.name, p.pid)).strong());
                            ui.separator();
                            if ui.button(tr("Application details")).clicked() { self.open_application(p.clone()); ui.close_menu(); }
                            if ui.button(tr("Properties")).clicked() { self.properties = Some(p.clone()); ui.close_menu(); }
                            if ui.button(tr("End task…")).clicked() {
                                self.selected = Some(key);
                                self.ask_end();
                                ui.close_menu();
                            }
                            if ui.button(tr(if p.status == "Suspended" { "Resume" } else { "Suspend" })).clicked() {
                                self.process_action(
                                    p,
                                    if p.status == "Suspended" { ProcessAction::Signal(libc::SIGCONT) } else { ProcessAction::Signal(libc::SIGSTOP) },
                                );
                                ui.close_menu();
                            }
                            if ui.button(tr("Copy command line")).clicked() {
                                ui.ctx().copy_text(p.command.clone());
                                ui.close_menu();
                            }
                            if ui.button(tr("Go to details")).clicked() {
                                self.switch(Page::Details);
                                self.selected = Some(key);
                                self.search = p.pid.to_string();
                                ui.close_menu();
                            }
                        });
                    }
                });
            });
        if let Some((key, (width, finished))) = resize {
            self.settings.column_widths.insert(format!("{table}/{key}"), width);
            ui.ctx().request_repaint();
            if finished {
                self.apply_settings(ui.ctx());
            }
        }
        if let Some((source, target)) = reorder {
            let columns = if details { &mut self.settings.detail_columns } else { &mut self.settings.columns };
            process_view::move_column(columns, source, target);
            self.apply_settings(ui.ctx());
        }
    }
    fn performance(&mut self, ui: &mut Ui) {
        let data = self.data.clone();
        if let Resource::Network(i) = self.resource {
            if data.sampled > 0 && !data.networks.get(i).is_some_and(|n| n.visible(self.settings.hide_virtual_networks)) {
                self.resource =
                    data.networks.iter().position(|n| n.visible(self.settings.hide_virtual_networks)).map(Resource::Network).unwrap_or(Resource::Cpu);
            }
        }
        let hide_virtual_networks = self.settings.hide_virtual_networks;
        let bounds = ui.available_rect_before_wrap();
        let side = Rect::from_min_size(bounds.min, vec2(230.0, bounds.height()));
        ui.scope_builder(UiBuilder::new().max_rect(side).layout(Layout::top_down(Align::Min)), |ui| {
            ScrollArea::vertical().id_salt("resource-list").max_height(bounds.height()).auto_shrink([false, false]).show(ui, |ui| {
                let sidebar_w = 230.0;
                ui.set_width(sidebar_w);
                ui.add_space(10.0);
                self.resource_item(
                    ui,
                    Resource::Cpu,
                    ("CPU", &format!("{:.0}%  {:.2} GHz", data.cpu, data.mhz / 1000.0)),
                    &data.cpu_total,
                    CYAN,
                    100.0,
                );
                self.resource_item(
                    ui,
                    Resource::Memory,
                    ("Memory", &format!("{} / {} ({:.0}%)", bytes(data.mem_used()), bytes(data.mem_total), data.mem_percent())),
                    &data.mem_history,
                    BLUE,
                    100.0,
                );
                for (i, d) in data.disks.iter().enumerate() {
                    self.resource_item(
                        ui,
                        Resource::Disk(i),
                        (&format!("{} {i} ({})", tr("Disk"), d.name), &format!("{}\n{:.0}%", tr(&d.kind), d.busy)),
                        &d.history,
                        GREEN,
                        100.0,
                    );
                }
                for (i, n) in data.networks.iter().enumerate().filter(|(_, n)| n.visible(hide_virtual_networks)) {
                    let max = n.rx_history.iter().chain(n.tx_history.iter()).copied().fold(1024.0, f32::max);
                    self.resource_item(
                        ui,
                        Resource::Network(i),
                        (n.kind(), &format!("{}\nS: {}  R: {}", n.name, bits(n.sent), bits(n.received))),
                        &n.rx_history,
                        PINK,
                        max,
                    );
                }
                for (i, g) in data.gpus.iter().enumerate() {
                    self.resource_item(
                        ui,
                        Resource::Gpu(i),
                        (
                            &format!("GPU {i}"),
                            &format!("{}\n{}", g.name, g.busy.map(|v| format!("{v:.0}%")).unwrap_or_else(|| "Not reported".into())),
                        ),
                        &g.history,
                        PURPLE,
                        100.0,
                    );
                }
                if data.gpus.is_empty() {
                    self.resource_item(ui, Resource::Gpu(0), ("GPU", "No GPU exposed by driver"), &Series::new(), PURPLE, 100.0);
                }
            });
        });
        let main = Rect::from_min_max(bounds.min + vec2(255.0, 0.0), bounds.max - vec2(25.0, 0.0));
        ui.scope_builder(UiBuilder::new().max_rect(main).layout(Layout::top_down(Align::Min)), |ui| {
            ui.set_width(main.width());
            ui.add_space(20.0);
            match self.resource {
                Resource::Cpu => {
                    self.perf_heading(ui, "CPU", &data.cpu_name);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("% Utilization")).size(13.0).color(muted(ui.ctx())));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.checkbox(&mut self.logical, tr("Logical processors"));
                        });
                    });
                    if self.settings.low_memory {
                        self.low_memory_graph(ui);
                    } else {
                        let count = if self.logical { data.cores.clamp(1, 64) } else { 1 };
                        let columns = if count == 1 {
                            1
                        } else if count <= 4 {
                            2
                        } else {
                            4
                        };
                        let rows = count.div_ceil(columns);
                        let size = vec2(ui.available_width(), (ui.available_height() - 190.0).max(180.0));
                        let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                        for i in 0..count {
                            let w = (r.width() - 5.0 * (columns - 1) as f32) / columns as f32;
                            let h = (r.height() - 5.0 * (rows - 1) as f32) / rows as f32;
                            let cr =
                                Rect::from_min_size(r.min + vec2((i % columns) as f32 * (w + 5.0), (i / columns) as f32 * (h + 5.0)), vec2(w, h));
                            let history = if self.logical { data.cpu_history.get(i).unwrap_or(&data.cpu_total) } else { &data.cpu_total };
                            graph(ui, cr, history, 100.0, CYAN, true, true);
                        }
                        if data.cores > 64 && self.logical {
                            ui.label(tr("Showing the first 64 logical processors."));
                        }
                        self.time_axis(ui);
                    }
                    ui.add_space(12.0);
                    ui.horizontal_top(|ui| {
                        metric(ui, "Utilization", format!("{:.0}%", data.cpu));
                        ui.add_space(22.0);
                        metric(ui, "Speed", if data.mhz > 0.0 { format!("{:.2} GHz", data.mhz / 1000.0) } else { "Not reported".into() });
                        ui.add_space(24.0);
                        ui.vertical(|ui| {
                            ui.label(format!("{}: {}", tr("Logical processors"), data.cores));
                            ui.label(format!("{}: {}", tr("Architecture"), std::env::consts::ARCH));
                            ui.label(format!("{}: {:.1} ms", tr("Collector time"), data.scan_ms));
                        });
                    });
                    ui.horizontal(|ui| {
                        metric(ui, "Processes", data.processes.len().to_string());
                        ui.add_space(20.0);
                        metric(ui, "Threads", data.threads.to_string());
                        ui.add_space(20.0);
                        metric(
                            ui,
                            "Up time",
                            format!("{}:{:02}:{:02}:{:02}", data.uptime / 86400, data.uptime / 3600 % 24, data.uptime / 60 % 60, data.uptime % 60),
                        );
                    });
                }
                Resource::Memory => {
                    self.perf_heading(ui, "Memory", &bytes(data.mem_total));
                    ui.label(RichText::new(tr("Memory usage")).color(muted(ui.ctx())));
                    self.large_graph(ui, &data.mem_history, 100.0, BLUE, 235.0);
                    self.time_axis(ui);
                    ui.add_space(12.0);
                    ui.label(tr("Memory composition"));
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::hover());
                    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, BLUE), StrokeKind::Inside);
                    ui.painter().rect_filled(
                        Rect::from_min_size(r.min, vec2(r.width() * data.mem_percent() / 100.0, r.height())),
                        0.0,
                        BLUE.gamma_multiply(0.25),
                    );
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        metric(ui, "In use", bytes(data.mem_used()));
                        ui.add_space(35.0);
                        metric(ui, "Available", bytes(data.mem_available));
                        ui.add_space(35.0);
                        metric(ui, "Cached", bytes(data.mem_cached));
                    });
                    ui.add_space(15.0);
                    ui.horizontal(|ui| {
                        metric(ui, "Swap used", bytes(data.swap_total.saturating_sub(data.swap_free)));
                        ui.add_space(35.0);
                        metric(ui, "Swap total", bytes(data.swap_total));
                    });
                }
                Resource::Disk(i) => {
                    if let Some(d) = data.disks.get(i) {
                        self.perf_heading(ui, &format!("{} {i} ({})", tr("Disk"), d.name), &d.model);
                        ui.label(tr("Active time"));
                        self.large_graph(ui, &d.history, 100.0, GREEN, 190.0);
                        self.time_axis(ui);
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            metric(ui, "Active time", format!("{:.0}%", d.busy));
                            ui.add_space(35.0);
                            metric(ui, "Read speed", rate(d.read_rate));
                            ui.add_space(35.0);
                            metric(ui, "Write speed", rate(d.write_rate));
                        });
                        ui.add_space(15.0);
                        ui.label(format!(
                            "{}: {}     {}: {}     {}: /dev/{}",
                            tr("Capacity"),
                            bytes(d.size),
                            tr("Type"),
                            tr(&d.kind),
                            tr("Device"),
                            d.name
                        ));
                    }
                }
                Resource::Network(i) => {
                    if let Some(n) = data.networks.get(i) {
                        self.perf_heading(ui, n.kind(), &n.name);
                        ui.label(RichText::new(tr("Throughput")).color(muted(ui.ctx())));
                        let max = n.rx_history.iter().chain(n.tx_history.iter()).copied().fold(1024.0, f32::max) * 1.1;
                        if self.settings.low_memory {
                            self.low_memory_graph(ui);
                        } else {
                            let (r, _) =
                                ui.allocate_exact_size(vec2(ui.available_width(), (ui.available_height() - 160.0).max(160.0)), Sense::hover());
                            graph(ui, r, &n.rx_history, max, PINK, true, true);
                            graph(ui, r, &n.tx_history, max, PINK.gamma_multiply(0.7), false, false);
                        }
                        self.time_axis(ui);
                        ui.add_space(16.0);
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                metric(ui, "Send", bits(n.sent));
                                metric(ui, "Receive", bits(n.received));
                            });
                            ui.add_space(40.0);
                            ui.vertical(|ui| {
                                ui.label(format!("{}: {}", tr("Adapter name"), n.name));
                                ui.label(format!("{}: {}", tr("Connection"), tr(&n.state)));
                                ui.label(format!("{}: {}", tr("MAC address"), n.mac));
                                ui.label(format!("{}: {}", tr("Received"), bytes(n.total_received)));
                                ui.label(format!("{}: {}", tr("Sent"), bytes(n.total_sent)));
                            });
                        });
                    }
                }
                Resource::Gpu(i) => self.gpu_ui(ui, &data, i),
            }
        });
        ui.allocate_rect(bounds, Sense::hover());
    }
    fn gpu_ui(&self, ui: &mut Ui, data: &Snapshot, index: usize) {
        let Some(g) = data.gpus.get(index) else {
            self.perf_heading(ui, "GPU", tr("No compatible device detected"));
            self.unavailable_panel(
                ui,
                "No GPU telemetry is available on this Linux system.\nThe GPU page uses real driver data when the device exposes it.",
            );
            return;
        };
        self.perf_heading(ui, "GPU", &g.name);
        if !g.note.is_empty() {
            ui.label(RichText::new(tr(&g.note)).color(muted(ui.ctx())));
            ui.add_space(8.0);
        }
        if self.settings.low_memory {
            self.low_memory_graph(ui);
        } else {
            let width = (ui.available_width() - 12.0) / 2.0;
            for pair in [
                [("GPU utilization", &g.history, g.busy), ("Video Encode", &g.encode_history, g.encoder)],
                [
                    ("Video Decode", &g.decode_history, g.decoder),
                    ("Dedicated GPU memory", &g.memory_history, g.used.zip(g.total).map(|(u, t)| u as f32 / t.max(1) as f32 * 100.0)),
                ],
            ] {
                ui.horizontal(|ui| {
                    for (label, series, value) in pair {
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            ui.horizontal(|ui| {
                                ui.label(tr(label));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(value.map(|v| format!("{v:.0}%")).unwrap_or_else(|| "—".into()));
                                });
                            });
                            let (r, _) = ui.allocate_exact_size(vec2(width, 160.0), Sense::hover());
                            graph(ui, r, series, 100.0, PURPLE, true, true);
                            if value.is_none() {
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, tr("Not reported"), FontId::proportional(14.0), muted(ui.ctx()));
                            }
                        });
                    }
                });
                ui.add_space(10.0);
            }
            self.time_axis(ui);
        }
        ui.add_space(15.0);
        ui.horizontal_wrapped(|ui| {
            metric(ui, "Utilization", g.busy.map(|n| format!("{n:.0}%")).unwrap_or_else(|| "Not reported".into()));
            ui.add_space(20.0);
            metric(
                ui,
                "Dedicated GPU memory",
                g.used.zip(g.total).map(|(u, t)| format!("{} / {}", bytes(u), bytes(t))).unwrap_or_else(|| "Not reported".into()),
            );
            ui.add_space(20.0);
            metric(ui, "Temperature", g.temperature.map(|n| format!("{n:.0} °C")).unwrap_or_else(|| "Not reported".into()));
            ui.add_space(20.0);
            if let Some(watts) = g.power {
                metric(ui, "Power", format!("{watts:.1} W"));
            }
        });
        ui.add_space(12.0);
        ui.label(format!("{}: {}", tr("Source"), g.source));
        if !g.driver.is_empty() {
            ui.label(format!("{}: {}", tr("Driver"), g.driver));
        }
        if !g.bus.is_empty() {
            ui.label(format!("PCI: {}", g.bus));
        }
    }
    fn perf_heading(&self, ui: &mut Ui, title: &str, subtitle: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(31.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(Label::new(RichText::new(subtitle).size(16.0)).truncate());
            });
        });
        ui.add_space(4.0);
    }
    fn resource_item(&mut self, ui: &mut Ui, id: Resource, labels: (&str, &str), history: &Series, color: Color32, max: f32) {
        let (title, subtitle) = labels;
        let (r, resp) = ui.allocate_exact_size(vec2(230.0, 77.0), Sense::click());
        if self.resource == id || resp.hovered() {
            ui.painter().rect_filled(r, 0.0, card(ui.ctx()));
        }
        let gr = Rect::from_min_size(r.min + vec2(12.0, 12.0), vec2(66.0, 50.0));
        if !self.settings.low_memory {
            graph(ui, gr, history, max, color, false, true);
        } else {
            ui.painter().rect_stroke(gr, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
        }
        let clip = ui.painter().with_clip_rect(Rect::from_min_max(pos2(r.left() + 88.0, r.top()), r.max));
        text(&clip, pos2(r.left() + 89.0, r.top() + 21.0), title, 18.0, fg(ui.ctx()));
        for (i, l) in subtitle.lines().enumerate() {
            text(&clip, pos2(r.left() + 89.0, r.top() + 42.0 + i as f32 * 16.0), l, 12.0, muted(ui.ctx()));
        }
        if resp.clicked() {
            self.resource = id;
        }
    }
    fn large_graph(&self, ui: &mut Ui, history: &Series, max: f32, color: Color32, reserve: f32) {
        if self.settings.low_memory {
            self.low_memory_graph(ui);
        } else {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), (ui.available_height() - reserve).max(160.0)), Sense::hover());
            graph(ui, r, history, max, color, true, true);
        }
    }
    fn low_memory_graph(&self, ui: &mut Ui) {
        self.unavailable_panel(ui, "Graphs are paused in low memory mode.\nCurrent readings remain available below.");
    }
    fn unavailable_panel(&self, ui: &mut Ui, message: &str) {
        Frame::NONE.fill(card(ui.ctx())).corner_radius(5.0).inner_margin(24.0).show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 48.0).max(150.0));
            ui.set_min_height(140.0);
            ui.add_space(32.0);
            ui.label(RichText::new(tr(message)).size(16.0).color(muted(ui.ctx())));
        });
    }
    fn time_axis(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} {}", self.data.cpu_total.len(), tr("samples · history since launch"))).size(12.0).color(muted(ui.ctx())),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(tr("now")).size(12.0).color(muted(ui.ctx())));
            });
        });
    }
    fn column_width(&self, table: &str, key: &str, default: f32) -> f32 {
        self.settings.column_widths.get(&format!("{table}/{key}")).copied().unwrap_or(default)
    }
    fn simple_table<F>(&self, ui: &mut Ui, id: &str, headers: &[(&str, f32)], count: usize, mut draw: F) -> Option<(String, (f32, bool))>
    where
        F: FnMut(&mut Ui, usize, Rect, &[Rect]),
    {
        let available = ui.available_width();
        let widths: Vec<_> = headers.iter().map(|(key, fraction)| self.column_width(id, key, (available * fraction).max(48.0))).collect();
        let width = widths.iter().sum::<f32>();
        let mut resize = None;
        ScrollArea::horizontal().id_salt((id, "horizontal")).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_min_width(width);
            let (r, _) = ui.allocate_exact_size(vec2(width, 36.0), Sense::hover());
            let mut x = r.left();
            for ((key, _), &w) in headers.iter().zip(&widths) {
                let c = Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height()));
                cell(ui, c, *key, false, muted(ui.ctx()), 14.0);
                ui.painter().vline(c.right(), r.y_range(), Stroke::new(1.0, line(ui.ctx())));
                if let Some(change) = column_resize(ui, id, key, c, w) {
                    resize = Some((*key, change));
                }
                x += w;
            }
            ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, line(ui.ctx())));
            ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show_rows(ui, 31.0, count, |ui, range| {
                let mut cells = vec![Rect::NOTHING; widths.len()];
                for i in range {
                    let (r, _) = ui.allocate_exact_size(vec2(width, 31.0), Sense::hover());
                    let mut x = r.left();
                    for (c, &w) in cells.iter_mut().zip(&widths) {
                        *c = Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height()));
                        x += w;
                    }
                    draw(ui, i, r, &cells);
                }
            });
        });
        resize.map(|(key, change)| (key.to_owned(), change))
    }
    fn apply_column_resize(&mut self, ctx: &Context, id: &str, resize: Option<(String, (f32, bool))>) {
        if let Some((key, (width, finished))) = resize {
            self.settings.column_widths.insert(format!("{id}/{key}"), width);
            ctx.request_repaint();
            if finished {
                self.apply_settings(ctx);
            }
        }
    }
    fn service_table(&mut self, ui: &mut Ui) {
        if self.services.is_empty() {
            ui.add_space(25.0);
            if self.busy {
                ui.spinner();
                ui.label(tr("Reading services…"));
            } else {
                self.unavailable_panel(ui, if self.services_error.is_empty() { "No systemd services were found." } else { &self.services_error });
            }
            return;
        }
        let query = self.search.to_lowercase();
        let rows: Vec<_> = self
            .services
            .iter()
            .enumerate()
            .filter(|(_, s)| s.name.to_lowercase().contains(&query) || s.description.to_lowercase().contains(&query))
            .map(|(i, s)| (i, s.clone()))
            .collect();
        let mut selected = self.service_selected;
        let resize = self.simple_table(
            ui,
            "services",
            &[("Name", 0.30), ("Description", 0.44), ("Status", 0.14), ("Group", 0.12)],
            rows.len(),
            |ui, index, r, cells| {
                let (i, s) = &rows[index];
                let response = ui.interact(r, ui.id().with(("svc", i)), Sense::click());
                if selected == Some(*i) || response.hovered() {
                    ui.painter().rect_filled(r, 0.0, card(ui.ctx()));
                }
                if response.clicked() {
                    selected = Some(*i);
                }
                for (t, &c) in [s.name.as_str(), s.description.as_str(), s.state.as_str(), s.scope.as_str()].iter().zip(cells) {
                    cell(ui, c, *t, false, fg(ui.ctx()), 14.0);
                }
            },
        );
        self.apply_column_resize(ui.ctx(), "services", resize);
        self.service_selected = selected;
    }
    fn startup_table(&mut self, ui: &mut Ui) {
        if self.startups.is_empty() {
            ui.add_space(25.0);
            self.unavailable_panel(ui, "No XDG autostart applications were found.\nDesktop startup entries will appear here when installed.");
            return;
        }
        let q = self.search.to_lowercase();
        let rows: Vec<_> = self
            .startups
            .iter()
            .enumerate()
            .filter(|(_, s)| s.name.to_lowercase().contains(&q) || s.command.to_lowercase().contains(&q))
            .map(|(i, s)| (i, s.clone()))
            .collect();
        let mut selected = self.startup_selected;
        let resize = self.simple_table(
            ui,
            "startup",
            &[("Name", 0.30), ("Command", 0.43), ("Status", 0.14), ("Source", 0.13)],
            rows.len(),
            |ui, index, r, cells| {
                let (i, s) = &rows[index];
                let response = ui.interact(r, ui.id().with(("startup", i)), Sense::click());
                if selected == Some(*i) || response.hovered() {
                    ui.painter().rect_filled(r, 0.0, card(ui.ctx()));
                }
                if response.clicked() {
                    selected = Some(*i);
                }
                for (t, &c) in
                    [s.name.as_str(), s.command.as_str(), if s.enabled { "Enabled" } else { "Disabled" }, s.source.as_str()].iter().zip(cells)
                {
                    cell(ui, c, *t, false, fg(ui.ctx()), 14.0);
                }
            },
        );
        self.apply_column_resize(ui.ctx(), "startup", resize);
        self.startup_selected = selected;
    }
    fn history_table(&mut self, ui: &mut Ui) {
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                RichText::new(format!(
                    "{} · {} min · {}",
                    tr("Resource usage since this app opened"),
                    self.session.elapsed().as_secs() / 60,
                    tr("session only")
                ))
                .color(muted(ui.ctx())),
            );
        });
        ui.add_space(12.0);
        let q = self.search.to_lowercase();
        let mut rows: Vec<_> = self.history.values().filter(|h| h.name.to_lowercase().contains(&q) || h.pid.to_string().contains(&q)).collect();
        rows.sort_by(|a, b| b.cpu_seconds.total_cmp(&a.cpu_seconds));
        let resize = self.simple_table(
            ui,
            "history",
            &[("Name", 0.38), ("PID", 0.13), ("CPU time", 0.18), ("Disk read", 0.16), ("Disk written", 0.15)],
            rows.len(),
            |ui, i, _r, cells| {
                let h = &rows[i];
                for (t, &c) in [h.name.clone(), h.pid.to_string(), format!("{:.2} s", h.cpu_seconds), bytes(h.read as u64), bytes(h.write as u64)]
                    .iter()
                    .zip(cells)
                {
                    cell(ui, c, t, false, fg(ui.ctx()), 14.0);
                }
            },
        );
        self.apply_column_resize(ui.ctx(), "history", resize);
    }
    fn users_table(&mut self, ui: &mut Ui) {
        let mut users: HashMap<String, (u32, f32, u64)> = HashMap::new();
        for p in &self.data.processes {
            let e = users.entry(p.user.clone()).or_default();
            e.0 += 1;
            e.1 += p.cpu;
            e.2 += p.memory;
        }
        let q = self.search.to_lowercase();
        let mut rows: Vec<_> = users.into_iter().filter(|(u, _)| u.to_lowercase().contains(&q)).collect();
        rows.sort_by(|a, b| b.1 .2.cmp(&a.1 .2));
        let resize = self.simple_table(
            ui,
            "users",
            &[("User", 0.42), ("Processes", 0.18), ("CPU", 0.18), ("Memory (sum of RSS)", 0.22)],
            rows.len(),
            |ui, i, _r, cells| {
                let (name, (count, cpu, memory)) = &rows[i];
                for (t, &c) in [name.clone(), count.to_string(), format!("{cpu:.1}%"), bytes(*memory)].iter().zip(cells) {
                    cell(ui, c, t, false, fg(ui.ctx()), 14.0);
                }
            },
        );
        self.apply_column_resize(ui.ctx(), "users", resize);
    }
    fn apply_settings(&mut self, ctx: &Context) {
        i18n::set_language(self.settings.language);
        ctx.send_viewport_cmd(ViewportCommand::Title(tr("Task Manager").into()));
        ctx.set_visuals(if self.settings.theme == "Light" { Visuals::light() } else { Visuals::dark() });
        ctx.style_mut(|s| {
            s.visuals.override_text_color = Some(if self.settings.theme == "Light" { Color32::from_gray(30) } else { Color32::from_gray(226) })
        });
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.settings.always_top { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal }));
        self.monitor.interval.store(
            if self.settings.interval == 0 {
                0
            } else if self.settings.low_memory {
                5000
            } else {
                self.settings.interval
            },
            Ordering::Relaxed,
        );
        if self.screenshot.is_none() {
            if let Err(e) = self.settings.save() {
                self.notify(format!("{}: {e}", tr("Could not save preferences")), true);
            }
        }
    }
    fn settings_ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut changed = false;
        ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.add_space(30.0);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 30.0).max(400.0));
                    ui.label(RichText::new(tr("Appearance")).strong().size(16.0));
                    ui.add_space(10.0);
                    Self::setting_card(ui, 7, "App theme", "Select which app theme to display", |ui| {
                        ComboBox::from_id_salt("theme").width(180.0).selected_text(tr(&self.settings.theme)).show_ui(ui, |ui| {
                            for theme in ["Dark", "Light"] {
                                changed |= ui.selectable_value(&mut self.settings.theme, theme.into(), tr(theme)).changed();
                            }
                        });
                    });
                    ui.add_space(26.0);
                    ui.label(RichText::new(tr("General")).strong().size(16.0));
                    ui.add_space(10.0);
                    Self::setting_card(ui, 0, "Language", "Choose the application language", |ui| {
                        ComboBox::from_id_salt("language").width(190.0).selected_text(tr(self.settings.language.label())).show_ui(ui, |ui| {
                            for language in [Language::Auto, Language::English, Language::Polish] {
                                changed |= ui.selectable_value(&mut self.settings.language, language, tr(language.label())).changed();
                            }
                        });
                    });
                    ui.add_space(3.0);
                    Self::setting_card(ui, 0, "Default start page", "Choose the page shown when opening Task Manager", |ui| {
                        ComboBox::from_id_salt("start").width(180.0).selected_text(tr(self.settings.start.title())).show_ui(ui, |ui| {
                            for page in Page::all().into_iter().filter(|p| *p != Page::Docker || self.docker.installed) {
                                changed |= ui.selectable_value(&mut self.settings.start, page, tr(page.title())).changed();
                            }
                        });
                    });
                    ui.add_space(3.0);
                    Self::setting_card(ui, 5, "Footer", "Choose which resource counters appear in the footer", |ui| {
                        ComboBox::from_id_salt("footer")
                            .width(180.0)
                            .selected_text(format!("{} / 6 {}", self.settings.footer.len(), tr("selected")))
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show_ui(ui, |ui| {
                                for item in FooterItem::all() {
                                    let mut visible = self.settings.footer.contains(&item);
                                    if ui.checkbox(&mut visible, tr(item.label())).on_hover_text(tr(item.description())).changed() {
                                        self.settings.footer.retain(|entry| *entry != item);
                                        if visible {
                                            self.settings.footer.push(item);
                                        }
                                        changed = true;
                                    }
                                }
                            });
                    });
                    ui.add_space(3.0);
                    Self::setting_card(ui, 3, "Real time update speed", "Choose how often system resource usage is refreshed", |ui| {
                        let speed = match self.settings.interval {
                            0 => "Paused",
                            500 => "High",
                            2000 => "Low",
                            _ => "Normal",
                        };
                        ComboBox::from_id_salt("speed").width(180.0).selected_text(tr(speed)).show_ui(ui, |ui| {
                            for (ms, label) in [(500, "High · 0.5 s"), (1000, "Normal · 1 s"), (2000, "Low · 2 s"), (0, "Paused")] {
                                changed |= ui.selectable_value(&mut self.settings.interval, ms, tr(label)).changed();
                            }
                        });
                    });
                    ui.add_space(3.0);
                    Self::setting_card(ui, 1, "Hide virtual network adapters", "Performance shows only physical Wi-Fi and Ethernet adapters", |ui| {
                        changed |= ui.checkbox(&mut self.settings.hide_virtual_networks, tr("Enabled")).changed();
                    });
                    ui.add_space(3.0);
                    Self::setting_card(ui, 1, "Low memory mode", "Refresh every 5 seconds, disable graphs and session history", |ui| {
                        changed |= ui.checkbox(&mut self.settings.low_memory, tr("Enabled")).changed();
                    });
                    ui.add_space(3.0);
                    Frame::NONE.fill(card(&ctx)).corner_radius(4.0).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        Self::setting_card(ui, 0, "Window management", "Set the default window behavior", |ui| {
                            ui.label(tr("^"));
                        });
                        ui.separator();
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.add_space(65.0);
                            changed |= ui.checkbox(&mut self.settings.always_top, tr("Always on top")).changed();
                        });
                        ui.add_space(16.0);
                    });
                    ui.add_space(12.0);
                    Frame::NONE.fill(card(&ctx)).corner_radius(4.0).show(ui, |ui| {
                        Self::setting_card(ui, 5, "Other options", "Process actions and resource collection", |ui| {
                            ui.label(tr("^"));
                        });
                        ui.separator();
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.add_space(65.0);
                            changed |= ui.checkbox(&mut self.settings.confirm, tr("Ask before ending a process")).changed();
                        });
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.add_space(65.0);
                            ui.label(RichText::new(tr("CPU is shown as a percentage of total system capacity.")).color(muted(&ctx)));
                        });
                        ui.add_space(15.0);
                    });
                    ui.add_space(26.0);
                    ui.label(RichText::new(tr("About")).strong().size(16.0));
                    ui.add_space(10.0);
                    Self::setting_card(ui, 1, "Task Manager for Linux", "Rust + egui · native rendering · no browser runtime", |ui| {
                        ui.label(env!("CARGO_PKG_VERSION"));
                    });
                    ui.add_space(12.0);
                    ui.label(RichText::new(tr("Process data: /proc · GPU: nvidia-smi / sysfs · Services: systemd")).small().color(muted(&ctx)));
                });
            });
            ui.add_space(30.0);
        });
        if changed {
            if self.settings.low_memory {
                self.history.clear();
            }
            self.apply_settings(&ctx);
        }
    }
    fn setting_card(ui: &mut Ui, kind: usize, title: &str, description: &str, control: impl FnOnce(&mut Ui)) {
        let ctx = ui.ctx().clone();
        Frame::NONE.fill(card(&ctx)).corner_radius(4.0).inner_margin(Margin::symmetric(22, 18)).show(ui, |ui| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 43.0), Sense::hover());
            icon(ui.painter(), pos2(r.left() + 14.0, r.center().y), kind, fg(&ctx));
            let labels = Rect::from_min_max(r.min + vec2(52.0, 0.0), pos2(r.right() - 225.0, r.bottom()));
            let painter = ui.painter().with_clip_rect(labels);
            text(&painter, pos2(labels.left(), r.top() + 12.0), title, 16.0, fg(&ctx));
            text(&painter, pos2(labels.left(), r.top() + 33.0), description, 13.0, muted(&ctx));
            let control_rect = Rect::from_min_max(pos2(r.right() - 210.0, r.top()), r.max);
            ui.scope_builder(UiBuilder::new().max_rect(control_rect).layout(Layout::right_to_left(Align::Center)), control);
        });
    }
    fn dialogs(&mut self, ctx: &Context) {
        self.application_properties(ctx);
        if demo::enabled() {
            self.authorization = None;
            self.run_dialog = false;
            self.kill_dialog = None;
            return;
        }
        if let Some(action) = self.authorization.clone() {
            let mut cancel = false;
            let mut authenticate = false;
            Window::new(tr("Administrator permission required"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(460.0)
                .show(ctx, |ui| {
                    ui.label(RichText::new(action.description()).strong());
                    ui.label(tr("This operation needs administrator permission. Authenticate using the system dialog to continue."));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        cancel = ui.button(tr("Cancel")).clicked();
                        authenticate = ui.button(tr("Authenticate…")).clicked();
                    });
                });
            if cancel {
                self.authorization = None;
            }
            if authenticate {
                if let PrivilegedAction::Update { key, .. } = &action {
                    if let Some(view) = self.application.as_mut().filter(|a| a.key() == *key) {
                        view.update = application_page::UpdateState::Updating;
                    }
                }
                self.authorization = None;
                self.busy = true;
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let result = action.authenticate();
                    let message = match action {
                        PrivilegedAction::Network { pid, start, .. } => Worker::Network(
                            (pid, start),
                            result.and_then(|s| s.trim().parse::<f32>().map_err(|_| "Invalid network measurement".into())),
                        ),
                        PrivilegedAction::Update { key, .. } => Worker::Updated(key, result.map(|_| ())),
                        _ => Worker::Action(result.map(|_| ())),
                    };
                    let _ = tx.send(message);
                    ctx.request_repaint();
                });
            }
        }
        if self.run_dialog {
            let mut open = true;
            let mut run = false;
            let mut cancel = false;
            Window::new(tr("Run new task"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(430.0)
                .show(ctx, |ui| {
                    ui.label(tr("Enter a command to start an application or open a terminal."));
                    ui.add_space(10.0);
                    ui.label(tr("Command"));
                    let response = ui.add(TextEdit::singleline(&mut self.command).hint_text(tr("e.g. xterm")).desired_width(f32::INFINITY));
                    if self.frames < 3 {
                        response.request_focus();
                    }
                    ui.label(RichText::new(tr("Runs with your current Linux user's permissions.")).small().color(muted(ctx)));
                    ui.add_space(16.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            cancel = true;
                        }
                        if ui.add(Button::new(tr("Run")).fill(ACCENT.gamma_multiply(0.5))).clicked()
                            || response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                        {
                            run = true;
                        }
                    });
                });
            if run {
                match launch(&self.command) {
                    Ok(()) => {
                        self.notify("Task started.", false);
                        self.command.clear();
                        self.run_dialog = false;
                    }
                    Err(e) => self.notify(e, true),
                }
            }
            if cancel || !open {
                self.run_dialog = false;
            }
        }
        if let Some(p) = self.kill_dialog.clone() {
            if !self.settings.confirm && !self.force {
                self.process_action(&p, ProcessAction::Signal(libc::SIGTERM));
                self.kill_dialog = None;
                return;
            }
            let mut cancel = false;
            let mut end = false;
            Window::new(tr("End this process?"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(440.0)
                .show(ctx, |ui| {
                    ui.label(RichText::new(format!("{} (PID {})", p.name, p.pid)).strong());
                    ui.label(tr("Unsaved work in this process may be lost."));
                    ui.add_space(12.0);
                    ui.checkbox(&mut self.force, tr("Force quit if unresponsive (SIGKILL)"));
                    ui.label(
                        RichText::new(tr(if self.force {
                            "The process will stop immediately."
                        } else {
                            "First ask the process to exit normally (SIGTERM)."
                        }))
                        .size(12.0)
                        .color(muted(ctx)),
                    );
                    ui.add_space(16.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            cancel = true;
                        }
                        if ui.add(Button::new(tr(if self.force { "Force quit" } else { "End task" })).fill(Color32::from_rgb(115, 43, 39))).clicked()
                        {
                            end = true;
                        }
                    });
                });
            if end {
                self.process_action(&p, ProcessAction::Signal(if self.force { libc::SIGKILL } else { libc::SIGTERM }));
                self.kill_dialog = None;
            }
            if cancel {
                self.kill_dialog = None;
            }
        }
        if let Some((message, error, at)) = &self.message {
            if at.elapsed() < Duration::from_secs(9) {
                let message = message.clone();
                let error = *error;
                let mut dismiss = false;
                Area::new(Id::new("notification")).anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -47.0)).order(Order::Foreground).show(ctx, |ui| {
                    Frame::window(&ctx.style()).fill(card(ctx)).stroke(Stroke::new(1.0, if error { ACCENT } else { GREEN })).inner_margin(16.0).show(
                        ui,
                        |ui| {
                            ui.set_max_width(470.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(if error { "!" } else { "✓" }).color(if error { ACCENT } else { GREEN }));
                                ui.label(tr(&message));
                                if ui.small_button("×").clicked() {
                                    dismiss = true;
                                }
                            });
                        },
                    );
                });
                if dismiss {
                    self.message = None;
                }
            }
        }
    }
}
impl eframe::App for TaskManager {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        self.update_data();
        if let Some(result) = self.docker.poll(ctx, self.page == Page::Docker, self.settings.interval, self.settings.low_memory) {
            match result {
                Ok(()) => self.notify("Action completed.", false),
                Err(e) => self.notify(e, true),
            }
        }
        if self.data.sampled > 0 {
            if let Some(pid) = self.application_request.take() {
                if let Some(process) = self.data.processes.iter().find(|p| p.pid == pid).cloned() {
                    self.open_application(process);
                } else {
                    self.notify("The process exited or changed. Select it again.", true);
                }
            }
        }
        if self.page == Page::Docker && self.docker.detection_finished() && !self.docker.installed {
            self.switch(Page::Apps);
        }
        self.load_application_metadata(ctx);
        if self.frames == 1 {
            ctx.send_viewport_cmd(ViewportCommand::Title(tr("Task Manager").into()));
            if self.page == Page::Services {
                self.load_services();
            }
            if self.page == Page::Startup {
                self.load_startups();
            }
            if self.screenshot.is_none() {
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.settings.always_top {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                }));
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Delete)) && !ctx.wants_keyboard_input() && self.properties.is_none() {
            if self.page == Page::Docker {
                self.docker.ask_delete();
            } else {
                self.ask_end();
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.run_dialog = false;
            self.kill_dialog = None;
            self.authorization = None;
            self.properties = None;
        }
        if ctx.input(|i| i.modifiers.alt && i.key_pressed(Key::ArrowLeft)) {
            if self.page == Page::Docker {
                self.docker.back();
            } else {
                self.close_application();
            }
        }
        self.titlebar(ctx);
        self.sidebar(ctx);
        self.statusbar(ctx);
        CentralPanel::default().frame(Frame::NONE.fill(bg(ctx))).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            self.toolbar(ui);
            if let Some(error) = &self.data.error {
                ui.colored_label(ACCENT, error);
            }
            if self.application.is_some() {
                self.application_page(ui);
                return;
            }
            match self.page {
                Page::Apps | Page::Processes => self.process_table(ui, false),
                Page::Details => self.process_table(ui, true),
                Page::Performance => self.performance(ui),
                Page::Services => self.service_table(ui),
                Page::Startup => self.startup_table(ui),
                Page::History => self.history_table(ui),
                Page::Users => self.users_table(ui),
                Page::Settings => self.settings_ui(ui),
                Page::Docker => self.docker.show(ui, &self.search, &mut self.settings),
            }
        });
        if std::mem::take(&mut self.docker.settings_changed) {
            self.apply_settings(ctx);
        }
        let screen = ctx.screen_rect();
        Area::new(Id::new("resize-grip")).fixed_pos(screen.max - vec2(16.0, 16.0)).order(Order::Foreground).show(ctx, |ui| {
            let (r, response) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::drag());
            response.clone().on_hover_cursor(CursorIcon::ResizeNwSe);
            for i in [4.0, 8.0, 12.0] {
                ui.painter().line_segment([r.right_bottom() - vec2(i, 2.0), r.right_bottom() - vec2(2.0, i)], Stroke::new(1.0, muted(ctx)));
            }
            if response.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(ResizeDirection::SouthEast));
            }
        });
        self.dialogs(ctx);
        self.docker.dialog(ctx);
        if let Some(path) = &self.screenshot {
            ctx.request_repaint_after(Duration::from_millis(16));
            if self.frames >= 120 && self.data.sampled > 0 && !self.screenshot_requested {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                self.screenshot_requested = true;
            }
            let mut saved = None;
            ctx.input(|i| {
                for e in &i.events {
                    if let Event::Screenshot { image, .. } = e {
                        let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                        if let Some(parent) = path.parent() {
                            if !parent.as_os_str().is_empty() {
                                let _ = fs::create_dir_all(parent);
                            }
                        }
                        saved = Some(image::save_buffer(path, &pixels, image.width() as u32, image.height() as u32, image::ColorType::Rgba8));
                    }
                }
            });
            if let Some(result) = saved {
                match result {
                    Ok(()) => println!("Screenshot saved: {}", path.display()),
                    Err(e) => {
                        eprintln!("Screenshot failed: {e}");
                        std::process::exit(1);
                    }
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        } else {
            ctx.request_repaint_after(Duration::from_secs(2));
        }
    }
}
fn main() -> eframe::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("linux-task-manager {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if demo::enabled() && args.iter().any(|a| a == "--privileged-action" || a == "--self-test") {
        eprintln!("Demo mode cannot be combined with system operations.");
        std::process::exit(2);
    }
    if let Some(i) = args.iter().position(|a| a == "--privileged-action") {
        match privilege::helper(&args[i + 1..]) {
            Ok(output) => {
                print!("{output}");
                std::process::exit(0)
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    if args.iter().any(|a| a == "--self-test") {
        model::self_test();
        return Ok(());
    }
    if args.iter().any(|a| a == "--help") {
        println!("Task Manager for Linux {}\n  --demo                  Fictional data; no system actions or saved settings\n  --version               Print version\n  --screenshot <file.png>  Capture after 120 frames, then exit\n  --page <apps|processes|performance|app-history|startup-apps|users|details|services|docker|settings>\n  --docker-view <projects|containers|images>\n  --docker-container <id> / --docker-image <id> / --docker-project <name>\n  --application <pid>      Open application details for a process\n  --resource <cpu|memory|disk|network|gpu>\n  --language <auto|en|pl>  Interface language\n  --low-memory             Start with reduced refresh and no graphs\n  --self-test              Check live procfs and terminate a disposable child", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let arg = |key: &str| args.iter().position(|a| a == key).and_then(|i| args.get(i + 1)).cloned();
    if args.iter().any(|a| a == "--screenshot") && arg("--screenshot").is_none() {
        eprintln!("--screenshot requires a PNG file path");
        std::process::exit(2);
    }
    let screenshot = arg("--screenshot").map(PathBuf::from);
    let page = arg("--page").map(|s| Page::from(&s));
    let resource = match arg("--resource").as_deref() {
        Some("memory") => Resource::Memory,
        Some("disk") => Resource::Disk(0),
        Some("network") => Resource::Network(0),
        Some("gpu") => Resource::Gpu(usize::MAX),
        _ => Resource::Cpu,
    };
    let low_memory = args.iter().any(|a| a == "--low-memory") || (screenshot.is_none() && Settings::load().low_memory);
    // Mesa's software fallback otherwise creates one worker per core. This
    // only affects llvmpipe and leaves hardware drivers and explicit overrides alone.
    if low_memory && std::env::var_os("LP_NUM_THREADS").is_none() {
        std::env::set_var("LP_NUM_THREADS", "1");
    }
    if screenshot.is_none() && !demo::enabled() {
        if let Err(error) = integration::register_desktop() {
            eprintln!("Could not register desktop launcher: {error}");
        }
    }
    let mut viewport = ViewportBuilder::default()
        .with_inner_size(if low_memory { [960.0, 680.0] } else { [1440.0, 1000.0] })
        .with_min_inner_size([960.0, 640.0])
        .with_decorations(false)
        .with_app_id(integration::APP_ID)
        .with_icon(integration::window_icon())
        .with_window_level(if screenshot.is_none() && Settings::load().always_top { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal })
        .with_title("Task Manager");
    if screenshot.is_some() || args.iter().any(|a| a == "--background") {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(WindowLevel::AlwaysOnBottom);
        }
    }
    let mut options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Glow, multisampling: 0, vsync: true, ..Default::default() };
    // winit's Wayland backend ignores WindowLevel. Ubuntu's XWayland supports
    // the EWMH above state, including changing it while the window is open.
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty()) {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        }));
    }
    #[cfg(target_os = "macos")]
    if screenshot.is_some() {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_activate_ignoring_other_apps(false);
        }));
    }
    let _ = &mut options;
    eframe::run_native("Task Manager", options, Box::new(move |cc| Ok(Box::new(TaskManager::new(cc, screenshot, page, resource)))))
}
