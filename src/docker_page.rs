//! Docker navigation and rendering. CLI work is always outside the paint thread.
use super::*;
use crate::docker::{self, Action, Backend, Container, Snapshot as DockerSnapshot, Target};

const CONTAINER_HEADINGS: &[&str] =
    &["Name", "Status", "CPU", "Memory", "Block I/O (R / W)", "Writable size", "Image", "Network I/O (Rx / Tx)", "PIDs", "Root filesystem size"];
const IMAGE_HEADINGS: &[&str] = &["Name", "Usage", "Size", "Running containers", "Containers", "Dependent images", "Layers", "Platform"];
fn headings(images: bool) -> &'static [&'static str] {
    if images {
        IMAGE_HEADINGS
    } else {
        CONTAINER_HEADINGS
    }
}
pub(crate) fn default_columns(images: bool) -> Vec<usize> {
    if images {
        (0..IMAGE_HEADINGS.len()).collect()
    } else {
        vec![0, 1, 2, 3, 5, 9, 6, 4, 7, 8]
    }
}
pub(crate) fn parse_columns(value: &str, images: bool) -> Vec<usize> {
    let mut columns = vec![];
    for key in value.split(',') {
        if let Some(index) = headings(images).iter().position(|h| *h == key) {
            if !columns.contains(&index) {
                columns.push(index);
            }
        }
    }
    if !columns.contains(&0) {
        columns.insert(0, 0);
    }
    columns
}
pub(crate) fn serialize_columns(columns: &[usize], images: bool) -> String {
    columns.iter().map(|&index| headings(images)[index]).collect::<Vec<_>>().join(",")
}
fn table_key(images: bool) -> &'static str {
    if images {
        "docker-images"
    } else {
        "docker-containers"
    }
}
fn column_menu(ui: &mut Ui, settings: &mut Settings, images: bool) -> bool {
    ui.label(RichText::new(tr("Select columns")).strong());
    ui.separator();
    ui.label(RichText::new(tr("Drag column headings to reorder; drag their edges to resize.")).small());
    let columns = if images { &mut settings.docker_image_columns } else { &mut settings.docker_columns };
    let mut changed = false;
    let mut order = columns.clone();
    order.extend((0..headings(images).len()).filter(|i| !columns.contains(i)));
    for index in order {
        ui.horizontal(|ui| {
            let mut visible = columns.contains(&index);
            if ui.add_enabled(index != 0, Checkbox::new(&mut visible, tr(headings(images)[index]))).changed() {
                columns.retain(|i| *i != index);
                if visible {
                    columns.push(index);
                }
                changed = true;
            }
            if let Some(at) = columns.iter().position(|i| *i == index) {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(at + 1 < columns.len(), Button::new(">")).on_hover_text(tr("Move right")).clicked() {
                        columns.swap(at, at + 1);
                        changed = true;
                    }
                    if ui.add_enabled(at > 0, Button::new("<")).on_hover_text(tr("Move left")).clicked() {
                        columns.swap(at, at - 1);
                        changed = true;
                    }
                });
            }
        });
    }
    ui.separator();
    if ui.button(tr("Reset columns")).clicked() {
        *columns = default_columns(images);
        settings.column_widths.retain(|key, _| !key.starts_with(&format!("{}/", table_key(images))));
        changed = true;
    }
    changed
}
#[derive(Clone, Copy)]
struct DragColumn {
    images: bool,
    index: usize,
}
type Collection = (Option<Target>, Result<DockerSnapshot, String>);
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum View {
    #[default]
    Projects,
    Containers,
    Images,
}
struct Pending {
    action: Action,
    ids: Vec<String>,
    names: Vec<String>,
    active: bool,
    force: bool,
}
pub(crate) struct DockerView {
    pub installed: bool,
    pub settings_changed: bool,
    detection: Option<mpsc::Receiver<Option<Backend>>>,
    backend: Option<Backend>,
    collection: Option<mpsc::Receiver<Collection>>,
    operation: Option<mpsc::Receiver<Result<(), String>>>,
    pub data: Arc<DockerSnapshot>,
    pub view: View,
    pub detail: Option<Target>,
    pub selected: Option<Target>,
    folded: HashSet<String>,
    pending: Option<Pending>,
    last_poll: Option<Instant>,
    pub error: String,
    fresh: bool,
    logs_target: Option<Target>,
    cpu: Series,
    memory: Series,
    io: Series,
    previous_io: Option<(u64, Instant)>,
    sort: usize,
    descending: bool,
}
impl Default for DockerView {
    fn default() -> Self {
        Self {
            installed: false,
            settings_changed: false,
            detection: None,
            backend: None,
            collection: None,
            operation: None,
            data: Arc::new(DockerSnapshot::default()),
            view: View::Projects,
            detail: None,
            selected: None,
            folded: HashSet::new(),
            pending: None,
            last_poll: None,
            error: String::new(),
            fresh: false,
            logs_target: None,
            cpu: Series::new(),
            memory: Series::new(),
            io: Series::new(),
            previous_io: None,
            sort: 0,
            descending: false,
        }
    }
}
impl DockerView {
    pub fn new(ctx: &Context) -> Self {
        if demo::enabled() {
            let mut view = Self { installed: true, data: Arc::new(docker::fixture()), fresh: true, ..Default::default() };
            view.cli_view();
            return view;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Backend::detect());
            ctx.request_repaint();
        });
        let mut view = Self { detection: Some(rx), ..Default::default() };
        view.cli_view();
        view
    }
    fn cli_view(&mut self) {
        let arg = |key: &str| std::env::args().skip_while(|a| a != key).nth(1);
        self.view = match arg("--docker-view").as_deref() {
            Some("containers") => View::Containers,
            Some("images") => View::Images,
            _ => View::Projects,
        };
        let target = arg("--docker-container")
            .map(Target::Container)
            .or_else(|| arg("--docker-image").map(Target::Image))
            .or_else(|| arg("--docker-project").map(Target::Project));
        if let Some(target) = target {
            self.open(target, std::env::args().any(|a| a == "--low-memory"));
        }
    }
    pub fn poll(&mut self, ctx: &Context, visible: bool, interval: u64, low: bool) -> Option<Result<(), String>> {
        if let Some(rx) = &self.detection {
            if let Ok(backend) = rx.try_recv() {
                self.installed = backend.is_some();
                self.backend = backend;
                self.detection = None;
            }
        }
        let mut action_result = None;
        if let Some(rx) = &self.operation {
            if let Ok(result) = rx.try_recv() {
                self.operation = None;
                self.last_poll = None;
                self.fresh = false;
                action_result = Some(result);
            }
        }
        if let Some(rx) = &self.collection {
            if let Ok((target, result)) = rx.try_recv() {
                self.collection = None;
                self.last_poll = if target == self.detail && action_result.is_none() { Some(Instant::now()) } else { None };
                match result {
                    Ok(data) => {
                        self.data = Arc::new(data);
                        self.logs_target = target.clone();
                        self.error.clear();
                        self.fresh = self.operation.is_none() && action_result.is_none();
                        if self.fresh {
                            self.reconcile_targets();
                        }
                        if target == self.detail && self.detail.is_some() && !low {
                            self.record_history();
                        }
                    }
                    Err(e) => {
                        self.error = e;
                        self.fresh = false;
                        self.cpu.clear();
                        self.memory.clear();
                        self.io.clear();
                        self.previous_io = None;
                    }
                }
            }
        }
        if visible && self.installed && !demo::enabled() {
            let due = self.last_poll.is_none_or(|at| at.elapsed() >= Duration::from_millis(interval.max(if low { 10_000 } else { 5_000 })));
            if due && (interval > 0 || self.last_poll.is_none()) && self.collection.is_none() && self.operation.is_none() {
                if let Some(backend) = self.backend.clone() {
                    let (tx, rx) = mpsc::sync_channel(1);
                    let target = self.detail.clone();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let data = backend.collect(target.as_ref());
                        let _ = tx.send((target, data));
                        ctx.request_repaint();
                    });
                    self.collection = Some(rx);
                    self.last_poll = Some(Instant::now());
                }
            }
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        action_result
    }
    fn record_history(&mut self) {
        let Some(target) = &self.detail else { return };
        let members = self.data.members(target);
        let (cpu, memory, io) = totals(&members);
        if let Some(value) = cpu {
            push(&mut self.cpu, value);
        } else {
            self.cpu.clear();
        }
        if let Some(value) = memory {
            push(&mut self.memory, value as f32);
        } else {
            self.memory.clear();
        }
        let now = Instant::now();
        if let Some(io) = io {
            if let Some((previous, at)) = self.previous_io {
                if io >= previous {
                    push(&mut self.io, (io - previous) as f32 / now.duration_since(at).as_secs_f32().max(0.1));
                } else {
                    self.io.clear();
                }
            }
            self.previous_io = Some((io, now));
        } else {
            self.previous_io = None;
            self.io.clear();
        }
    }
    pub fn open(&mut self, target: Target, low: bool) {
        self.selected = Some(target.clone());
        self.detail = Some(target);
        self.last_poll = None;
        self.cpu.clear();
        self.memory.clear();
        self.io.clear();
        self.previous_io = None;
        if demo::enabled() && !low {
            let (cpu, mem, _) = totals(&self.data.members(self.detail.as_ref().unwrap()));
            [self.cpu, self.memory, self.io] = demo::application_histories(cpu.unwrap_or(0.0), mem.unwrap_or(0), 12_000.0);
            self.logs_target = self.detail.clone();
        }
    }
    pub fn detection_finished(&self) -> bool {
        self.detection.is_none()
    }
    pub fn back(&mut self) {
        self.detail = None;
        self.cpu.clear();
        self.memory.clear();
        self.io.clear();
        self.previous_io = None;
        self.last_poll = None;
    }
    fn eligible(&self, action: Action) -> Vec<&Container> {
        self.selected
            .as_ref()
            .filter(|t| !matches!(t, Target::Image(_)))
            .map(|t| self.data.members(t))
            .unwrap_or_default()
            .into_iter()
            .filter(|c| match action {
                Action::Start => matches!(c.status.as_str(), "created" | "exited"),
                Action::Stop | Action::Kill => c.active(),
                Action::Restart => c.running(),
                Action::RemoveContainer => c.status != "removing",
                Action::RemoveImage => false,
            })
            .collect()
    }
    fn delete_action(&self) -> Action {
        if matches!(self.selected, Some(Target::Image(_))) {
            Action::RemoveImage
        } else {
            Action::RemoveContainer
        }
    }
    pub(crate) fn ask_delete(&mut self) {
        self.ask_action(self.delete_action(), true);
    }
    fn pending_action(&self, action: Action) -> Option<Pending> {
        if action == Action::RemoveImage {
            let Some(Target::Image(id)) = &self.selected else { return None };
            let image = self.data.images.iter().find(|i| i.id == *id)?;
            return Some(Pending {
                action,
                ids: vec![image.id.clone()],
                names: vec![format!("{} · {}", image.tags.join(", "), image.id)],
                active: false,
                force: false,
            });
        }
        let containers = self.eligible(action);
        if containers.is_empty() {
            return None;
        }
        Some(Pending {
            action,
            ids: containers.iter().map(|c| c.id.clone()).collect(),
            names: containers.iter().map(|c| format!("{} · {}", c.name, c.id)).collect(),
            active: containers.iter().any(|c| c.active()),
            force: false,
        })
    }
    fn can_action(&self, action: Action) -> bool {
        self.fresh
            && self.pending.is_none()
            && self.operation.is_none()
            && !demo::enabled()
            && if action == Action::RemoveImage {
                matches!(&self.selected, Some(Target::Image(id)) if self.data.images.iter().any(|i| i.id == *id))
            } else {
                !self.eligible(action).is_empty()
            }
    }
    fn reconcile_targets(&mut self) {
        let exists = |target: &Target| match target {
            Target::Image(id) => self.data.images.iter().any(|i| i.id == *id),
            _ => !self.data.members(target).is_empty(),
        };
        let detail_missing = self.detail.as_ref().is_some_and(|t| !exists(t));
        if self.selected.as_ref().is_some_and(|t| !exists(t)) {
            self.selected = None;
        }
        if detail_missing {
            self.back();
        }
    }
    pub(crate) fn ask_action(&mut self, action: Action, confirm: bool) {
        if !self.can_action(action) || self.backend.is_none() {
            return;
        }
        let Some(pending) = self.pending_action(action) else { return };
        if action.requires_confirmation(confirm) {
            self.pending = Some(pending);
        } else {
            self.execute(pending);
        }
    }
    fn execute(&mut self, pending: Pending) {
        if demo::enabled()
            || !self.fresh
            || self.operation.is_some()
            || (pending.active && pending.action == Action::RemoveContainer && !pending.force)
        {
            return;
        }
        let Some(backend) = self.backend.clone() else { return };
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(backend.action(pending.action, &pending.ids, pending.force));
        });
        self.operation = Some(rx);
        self.fresh = false;
    }
    pub fn toolbar(&mut self, ui: &mut Ui, settings: &mut Settings) {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, line(ui.ctx())));
        let mut bar = ui
            .new_child(UiBuilder::new().id_salt("docker-toolbar").max_rect(r.shrink2(vec2(16.0, 8.0))).layout(Layout::left_to_right(Align::Center)));
        if self.detail.is_some() && bar.add(quiet_button(tr("<  Back"))).clicked() {
            self.back();
        }
        bar.label(RichText::new("Docker").strong().size(17.0));
        bar.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add_enabled(self.collection.is_none() && self.operation.is_none() && !demo::enabled(), quiet_button(tr("Refresh"))).clicked() {
                self.last_poll = None;
            }
            for action in [self.delete_action(), Action::Kill, Action::Restart, Action::Stop, Action::Start] {
                let enabled = self.can_action(action);
                if ui.add_enabled(enabled, quiet_button(tr(action.label()))).clicked() {
                    self.ask_action(action, settings.confirm);
                }
            }
            ui.menu_button(tr("Columns"), |ui| {
                self.settings_changed |= column_menu(ui, settings, self.detail.is_none() && self.view == View::Images);
            });
            if self.collection.is_some() || self.operation.is_some() {
                ui.spinner();
            }
        });
    }
    pub fn dialog(&mut self, ctx: &Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.pending = None;
        }
        let Some(pending) = &mut self.pending else { return };
        let mut cancel = false;
        let mut proceed = false;
        Window::new(tr("Docker action")).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(format!(
                "{} · {} {}",
                tr(pending.action.label()),
                pending.ids.len(),
                tr(if pending.action == Action::RemoveImage { "images" } else { "containers" })
            ));
            if pending.action == Action::Kill {
                ui.colored_label(ACCENT, tr("Force stop sends SIGKILL immediately. Unsaved data may be lost."));
            } else if pending.action == Action::Stop {
                ui.label(tr("Wait up to 10 seconds, then force stop any remaining containers."));
            } else if pending.action == Action::RemoveContainer {
                ui.label(tr("Permanently delete these containers and their writable data? Volumes and images are kept."));
                if pending.active {
                    ui.colored_label(ACCENT, tr("Force deletion sends SIGKILL immediately. Unsaved data will be lost."));
                    ui.checkbox(&mut pending.force, tr("Force delete running containers"));
                }
            } else if pending.action == Action::RemoveImage {
                ui.label(tr("Permanently delete this image? Docker may refuse images with container references, dependent images or multiple tags."));
            }
            ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for name in &pending.names {
                    ui.label(name);
                }
            });
            ui.horizontal(|ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                let enabled = !demo::enabled() && self.fresh && (pending.action != Action::RemoveContainer || !pending.active || pending.force);
                proceed = ui.add_enabled(enabled, Button::new(tr(pending.action.label())).fill(ACCENT)).clicked();
            });
        });
        if cancel {
            self.pending = None;
        } else if proceed {
            if let Some(pending) = self.pending.take() {
                self.execute(pending);
            }
        }
    }
    pub fn show(&mut self, ui: &mut Ui, search: &str, settings: &mut Settings) {
        if !self.installed {
            Frame::NONE.inner_margin(24.0).show(ui, |ui| {
                ui.label(tr(if self.detection.is_some() { "Detecting Docker…" } else { "Docker is not installed." }));
            });
            return;
        }
        if !self.error.is_empty() {
            Frame::NONE.inner_margin(16.0).show(ui, |ui| {
                ui.colored_label(ACCENT, tr("Docker is unavailable. Check that the daemon is running and your user has socket access."));
                ui.label(&self.error);
                ui.label(tr("Previously collected data is stale. Actions are disabled until refresh succeeds."));
            });
        }
        if !self.data.warning.is_empty() {
            ui.colored_label(ACCENT, &self.data.warning);
        }
        if self.detail.is_some() {
            self.details(ui, search, settings);
            return;
        }
        Frame::NONE.inner_margin(Margin::symmetric(20, 10)).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (view, label, count) in [
                    (View::Projects, "Projects", self.data.projects().keys().filter(|k| !k.is_empty()).count()),
                    (View::Containers, "Containers", self.data.containers.len()),
                    (View::Images, "Images", self.data.images.len()),
                ] {
                    if ui.selectable_label(self.view == view, format!("{} ({count})", tr(label))).clicked() {
                        self.view = view;
                        self.selected = None;
                        self.sort = 0;
                        self.descending = false;
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.backend.as_ref().map(|b| b.context.as_str()).unwrap_or("Demo · fictional data"))
                            .small()
                            .color(muted(ui.ctx())),
                    );
                });
            });
        });
        let members: Vec<_> = self.data.containers.iter().collect();
        let running = members.iter().filter(|c| c.active()).count();
        let (cpu, memory, _) = totals(&members);
        Frame::NONE.fill(shell(ui.ctx())).inner_margin(Margin::symmetric(24, 14)).show(ui, |ui| {
            ui.columns(4, |cols| {
                metric(&mut cols[0], "Running containers", running.to_string());
                metric(&mut cols[1], "Stopped containers", (members.len() - running).to_string());
                metric(&mut cols[2], "Total Docker CPU", percent(cpu));
                metric(&mut cols[3], "Total Docker memory", size(memory));
            });
            ui.add_space(16.0);
            ui.columns(4, |cols| {
                metric(&mut cols[0], "Writable size", size(sum_sizes(members.iter().map(|c| c.size))));
                metric(&mut cols[1], "Root filesystem size", size(sum_sizes(members.iter().map(|c| c.root_size))));
                metric(&mut cols[2], "Images", self.data.images.len().to_string());
                metric(&mut cols[3], "Images size", size(sum_sizes(self.data.images.iter().map(|i| i.size))));
            });
        });
        Frame::NONE.inner_margin(Margin::symmetric(20, 10)).show(ui, |ui| {
            ui.label(RichText::new(tr("Double-click to open details")).small().color(muted(ui.ctx())));
            ui.label(
                RichText::new(tr(
                    "Docker totals include all containers, regardless of search. Sizes exclude volumes; root filesystems and images share layers.",
                ))
                .small()
                .color(muted(ui.ctx())),
            );
        });
        if self.collection.is_some() && !self.fresh && self.data.containers.is_empty() && self.data.images.is_empty() {
            ui.label(tr("Reading Docker inventory…"));
        }
        let mut rows = vec![];
        let query = search.to_lowercase();
        match self.view {
            View::Projects => {
                rows = project_rows(&self.data, &query, &self.folded, self.sort, self.descending);
            }
            View::Containers => {
                for c in &self.data.containers {
                    if container_matches(c, &query) {
                        rows.push(Row::container(c, false));
                    }
                }
            }
            View::Images => {
                for image in &self.data.images {
                    if !query.is_empty() && !image.tags.join(" ").to_lowercase().contains(&query) && !image.id.contains(&query) {
                        continue;
                    }
                    rows.push(Row::image(image, &self.data));
                }
            }
        }
        if self.view != View::Projects {
            sort_rows(&mut rows, self.sort, self.descending);
        }
        self.table(ui, rows, self.view == View::Images, settings);
    }
    fn table(&mut self, ui: &mut Ui, rows: Vec<Row>, images: bool, settings: &mut Settings) {
        let (low, confirm) = (settings.low_memory, settings.confirm);
        let headings = headings(images);
        let columns = if images { settings.docker_image_columns.clone() } else { settings.docker_columns.clone() };
        let defaults: &[f32] = if images {
            &[290.0, 100.0, 110.0, 150.0, 100.0, 155.0, 80.0, 130.0]
        } else {
            &[215.0, 120.0, 90.0, 110.0, 170.0, 160.0, 165.0, 180.0, 70.0, 180.0]
        };
        let table = table_key(images);
        let widths: Vec<_> =
            columns.iter().map(|&i| settings.column_widths.get(&format!("{table}/{}", headings[i])).copied().unwrap_or(defaults[i])).collect();
        let snapshot = self.data.clone();
        let members: Vec<_> = self.detail.as_ref().map(|target| snapshot.members(target)).unwrap_or_else(|| snapshot.containers.iter().collect());
        let summary = if images {
            vec![
                snapshot.images.len().to_string(),
                String::new(),
                size(sum_sizes(snapshot.images.iter().map(|i| i.size))),
                members.iter().filter(|c| c.active()).count().to_string(),
                members.len().to_string(),
                String::new(),
                String::new(),
                String::new(),
            ]
        } else {
            Row::project("", &members, false).cells
        };
        let mut resize = None;
        let mut reorder = None;
        let table_id = ("docker-table", images, self.detail.clone().map(|t| format!("{t:?}")));
        let name_offset: f32 = columns.iter().zip(&widths).take_while(|(i, _)| **i != 0).map(|(_, w)| w).sum();
        let mut open = None;
        let mut context_action = None;
        ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
        ScrollArea::horizontal()
            .id_salt(&table_id)
            .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysVisible)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let width = widths.iter().sum::<f32>().max(ui.available_width());
                ui.set_min_width(width);
                let (header, _) = ui.allocate_exact_size(vec2(width, 61.0), Sense::hover());
                ui.painter().hline(header.x_range(), header.bottom(), Stroke::new(1.0, line(ui.ctx())));
                let mut x = header.left();
                for (&i, &w) in columns.iter().zip(&widths) {
                    let r = Rect::from_min_size(pos2(x, header.top()), vec2(w, header.height()));
                    let numeric = numeric_column(images, i);
                    let selected = self.sort == i;
                    if selected {
                        ui.painter().rect_filled(r, 0.0, card(ui.ctx()));
                    }
                    ui.painter().vline(r.left(), r.y_range(), Stroke::new(1.0, line(ui.ctx())));
                    if numeric || i == 0 {
                        let total = if i == 0 { if images { snapshot.images.len() } else { members.len() }.to_string() } else { summary[i].clone() };
                        cell(ui, Rect::from_min_size(r.min, vec2(w, 30.0)), total, numeric, fg(ui.ctx()), 20.0);
                    }
                    if selected {
                        let c = pos2(if numeric { r.left() + 13.0 } else { r.center().x }, r.top() + 8.0);
                        let offsets = if self.descending {
                            [vec2(-3.0, -1.5), vec2(0.0, 1.5), vec2(3.0, -1.5)]
                        } else {
                            [vec2(-3.0, 1.5), vec2(0.0, -1.5), vec2(3.0, 1.5)]
                        };
                        ui.painter().add(Shape::line(offsets.iter().map(|d| c + *d).collect(), Stroke::new(1.2, muted(ui.ctx()))));
                    }
                    cell(ui, Rect::from_min_max(pos2(r.left(), r.bottom() - 28.0), r.max), tr(headings[i]), numeric, muted(ui.ctx()), 14.0);
                    let response = ui.interact(r.shrink2(vec2(4.0, 0.0)), ui.id().with(("docker-sort", i)), Sense::click_and_drag());
                    if let Some(change) = column_resize(ui, table, headings[i], r, w) {
                        resize = Some((i, change));
                    }
                    response.dnd_set_drag_payload(DragColumn { images, index: i });
                    if let Some(source) = response.dnd_hover_payload::<DragColumn>() {
                        if source.images == images && source.index != i {
                            ui.painter().vline(r.left(), r.y_range(), Stroke::new(3.0, ACCENT));
                        }
                    }
                    if let Some(source) = response.dnd_release_payload::<DragColumn>() {
                        if source.images == images && source.index != i {
                            reorder = Some((source.index, i));
                        }
                    }
                    response.clone().on_hover_text(tr("Drag edges to resize · Drag to move · Click to sort · Right-click for columns"));
                    response.clone().on_hover_text(tr(if images {
                        "Image sizes include shared layers; do not add them to estimate disk usage."
                    } else {
                        "CPU: 100% = one logical CPU. I/O counters are cumulative. — = not reported."
                    }));
                    if response.clicked() {
                        self.descending = if self.sort == i { !self.descending } else { numeric };
                        self.sort = i;
                    }
                    response.context_menu(|ui| {
                        self.settings_changed |= column_menu(ui, settings, images);
                    });
                    x += w;
                }
                if rows.is_empty() {
                    ui.add_space(24.0);
                    ui.label(tr("No matching Docker items."));
                }
                ScrollArea::vertical().id_salt((table_id, "rows")).auto_shrink([false, false]).show_rows(ui, 38.0, rows.len(), |ui, range| {
                    for index in range {
                        let row = &rows[index];
                        let (rect, _) = ui.allocate_exact_size(vec2(width, 38.0), Sense::hover());
                        let response = ui.interact(rect, ui.id().with(("docker-row", format!("{:?}", row.target))), Sense::click());
                        if self.selected.as_ref() == Some(&row.target) || response.hovered() {
                            ui.painter().rect_filled(rect, 0.0, card(ui.ctx()));
                        }
                        if row.project {
                            ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0, line(ui.ctx())));
                        }
                        let mut x = rect.left();
                        for (&i, &w) in columns.iter().zip(&widths) {
                            let value = &row.cells[i];
                            let bounds = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
                            ui.painter().vline(x, bounds.y_range(), Stroke::new(1.0, line(ui.ctx())));
                            if !images && i == 2 && !row.project && self.selected.as_ref() != Some(&row.target) {
                                if let Some(cpu) = row.values.get(i).copied().flatten().filter(|v| *v > 5.0) {
                                    let alpha = (25.0 + cpu.min(100.0) * 1.3) as u8;
                                    ui.painter().rect_filled(bounds.shrink(1.0), 0.0, Color32::from_rgba_unmultiplied(180, 50, 35, alpha));
                                }
                            }
                            let r = Rect::from_min_size(pos2(x + 14.0, rect.top()), vec2(w - 24.0, rect.height()));
                            let painter = ui.painter().with_clip_rect(r);
                            let indent = if i == 0 && row.child { 25.0 } else { 0.0 };
                            if i == 0 {
                                if row.project {
                                    let c = pos2(x + 22.0, rect.center().y);
                                    let points = if row.folded {
                                        [c + vec2(-2.0, -4.0), c + vec2(2.0, 0.0), c + vec2(-2.0, 4.0)]
                                    } else {
                                        [c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)]
                                    };
                                    painter.add(Shape::line(points.to_vec(), Stroke::new(1.5, muted(ui.ctx()))));
                                } else {
                                    icon(&painter, pos2(x + 25.0 + indent, rect.center().y), if images { 10 } else { 11 }, BLUE);
                                }
                            }
                            let left = x + if i == 0 { 44.0 + indent } else { 14.0 };
                            if i == 0 {
                                text(&painter, pos2(left, rect.center().y), value, 14.0, fg(ui.ctx()));
                            } else {
                                cell(ui, bounds, value, numeric_column(images, i), if i == 1 && row.active { GREEN } else { fg(ui.ctx()) }, 14.0);
                            }
                            x += w;
                        }
                        if response.clicked() {
                            self.selected = Some(row.target.clone());
                        }
                        if response.double_clicked() {
                            open = Some(row.target.clone());
                        }
                        if row.project
                            && response.clicked()
                            && response
                                .interact_pointer_pos()
                                .is_some_and(|p| p.x >= rect.left() + name_offset && p.x < rect.left() + name_offset + 38.0)
                        {
                            if let Target::Project(name) = &row.target {
                                if !self.folded.remove(name) {
                                    self.folded.insert(name.clone());
                                }
                            }
                        }
                        response.on_hover_text(row.cells.join(" · ")).context_menu(|ui| {
                            if ui.button(tr("Details")).clicked() {
                                open = Some(row.target.clone());
                                ui.close_menu();
                            }
                            self.selected = Some(row.target.clone());
                            ui.separator();
                            let actions = if images {
                                vec![Action::RemoveImage]
                            } else {
                                vec![Action::Start, Action::Stop, Action::Restart, Action::Kill, Action::RemoveContainer]
                            };
                            for action in actions {
                                if ui.add_enabled(self.can_action(action), Button::new(tr(action.label()))).clicked() {
                                    context_action = Some(action);
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
            });
        if let Some((i, (width, finished))) = resize {
            settings.column_widths.insert(format!("{table}/{}", headings[i]), width);
            self.settings_changed |= finished;
        }
        if let Some((source, target)) = reorder {
            let columns = if images { &mut settings.docker_image_columns } else { &mut settings.docker_columns };
            if let (Some(from), Some(to)) = (columns.iter().position(|i| *i == source), columns.iter().position(|i| *i == target)) {
                let value = columns.remove(from);
                columns.insert(to, value);
                self.settings_changed = true;
            }
        }
        if let Some(target) = open {
            self.open(target, low);
        }
        if let Some(action) = context_action {
            self.ask_action(action, confirm);
        }
    }
    fn details(&mut self, ui: &mut Ui, search: &str, settings: &mut Settings) {
        let low = settings.low_memory;
        let target = self.detail.clone().unwrap();
        let snapshot = self.data.clone();
        let members = snapshot.members(&target);
        let image = if let Target::Image(id) = &target { snapshot.images.iter().find(|i| i.id == *id) } else { None };
        let container = if matches!(&target, Target::Container(_)) { members.first().copied() } else { None };
        let title = match &target {
            Target::Container(_) => container.map(|c| c.name.as_str()).unwrap_or("Container removed"),
            Target::Image(_) => image.map(|i| i.name()).unwrap_or("Image removed"),
            Target::Project(name) => {
                if name.is_empty() {
                    tr("Standalone containers")
                } else {
                    name
                }
            }
        };
        ScrollArea::vertical().id_salt("docker-details").auto_shrink([false, false]).show(ui, |ui| {
            Frame::NONE.inner_margin(24.0).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().item_spacing = vec2(16.0, 12.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    icon(ui.painter(), r.center(), 10, BLUE);
                    ui.vertical(|ui| {
                        ui.heading(RichText::new(title).size(28.0));
                        ui.label(
                            RichText::new(tr(match &target {
                                Target::Container(_) => "Container details",
                                Target::Image(_) => "Image details",
                                Target::Project(name) if name.is_empty() => "Standalone containers",
                                Target::Project(_) => "Docker Compose project",
                            }))
                            .color(muted(ui.ctx())),
                        );
                    });
                });
                if let Some(image) = image {
                    ui.columns(4, |cols| {
                        metric(&mut cols[0], "Size", size(image.size));
                        metric(&mut cols[1], "Running containers", members.iter().filter(|c| c.active()).count().to_string());
                        metric(&mut cols[2], "Containers", members.len().to_string());
                        metric(&mut cols[3], "Dependent images", snapshot.images.iter().filter(|i| i.derived_from(image)).count().to_string());
                    });
                    Grid::new("image-information").num_columns(2).spacing(vec2(24.0, 12.0)).show(ui, |ui| {
                        info(ui, "Image ID", &image.id);
                        info(ui, "Tags", &image.tags.join(", "));
                        info(ui, "Digests", &image.digests.join(", "));
                        info(ui, "Created", &image.created);
                        info(ui, "Platform", &image.platform);
                        info(ui, "Parent image", &image.parent);
                    });
                    ui.label(
                        RichText::new(tr("Image sizes include shared layers; do not add them to estimate disk usage."))
                            .small()
                            .color(muted(ui.ctx())),
                    );
                    ui.label(RichText::new(tr("Layers")).strong());
                    ui.label(
                        RichText::new(tr("Dependent images share the complete base layer prefix; build ancestry is not always retained."))
                            .small()
                            .color(muted(ui.ctx())),
                    );
                    for (index, layer) in image.layers.iter().enumerate() {
                        ui.label(RichText::new(format!("{:02}  {}", index + 1, layer)).monospace().small());
                    }
                } else {
                    let (cpu, memory, io) = totals(&members);
                    ui.columns(4, |cols| {
                        metric(&mut cols[0], "CPU", percent(cpu));
                        metric(&mut cols[1], "Memory", size(memory));
                        metric(&mut cols[2], "Block I/O", size(io));
                        metric(&mut cols[3], "Writable size", size(sum_sizes(members.iter().map(|c| c.size))));
                    });
                    ui.label(
                        RichText::new(tr("CPU: 100% = one logical CPU. I/O counters are cumulative. — = not reported."))
                            .small()
                            .color(muted(ui.ctx())),
                    );
                    if low {
                        ui.label(tr("Graphs are disabled in low memory mode."));
                    } else {
                        ui.columns(3, |cols| {
                            for (i, label, samples, color, minimum) in [
                                (0, "CPU", &self.cpu, CYAN, 100.0),
                                (1, "Memory", &self.memory, BLUE, 1.0),
                                (2, "Disk I/O rate", &self.io, GREEN, 1024.0),
                            ] {
                                let ui = &mut cols[i];
                                ui.label(RichText::new(tr(label)).color(color));
                                let max = samples.iter().copied().fold(minimum, f32::max) * 1.1;
                                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 125.0), Sense::hover());
                                graph(ui, r, samples, max, color, true, true);
                                if samples.is_empty() {
                                    ui.painter().text(
                                        r.center(),
                                        Align2::CENTER_CENTER,
                                        tr("Not reported"),
                                        FontId::proportional(13.0),
                                        muted(ui.ctx()),
                                    );
                                }
                            }
                        });
                        ui.label(RichText::new(tr("Last 60 samples · history starts when this page opens")).small().color(muted(ui.ctx())));
                    }
                    if let Some(c) = container {
                        Frame::NONE.fill(card(ui.ctx())).corner_radius(5.0).inner_margin(16.0).show(ui, |ui| {
                            Grid::new("container-information").num_columns(2).spacing(vec2(24.0, 10.0)).show(ui, |ui| {
                                info(ui, "Container ID", &c.id);
                                info(ui, "Status", state_label(&c.status));
                                info(ui, "Health", &c.health);
                                ui.label(tr("Image"));
                                if ui.link(&c.image_name).clicked() {
                                    self.open(Target::Image(c.image.clone()), low);
                                }
                                ui.end_row();
                                ui.label(tr("Project"));
                                if !c.project.is_empty() {
                                    if ui.link(&c.project).clicked() {
                                        self.open(Target::Project(c.project.clone()), low);
                                    }
                                } else {
                                    ui.label(tr("Standalone containers"));
                                }
                                ui.end_row();
                                info(ui, "Service", &c.service);
                                info(ui, "Created", &c.created);
                                info(ui, "Started", &c.started);
                                info(ui, "Finished", &c.finished);
                                info(ui, "Exit code", &c.exit_code.map(|n| n.to_string()).unwrap_or_default());
                                info(ui, "Restart policy", &c.restart);
                                info(ui, "Memory limit", &size(c.stats.as_ref().and_then(|s| s.limit)));
                                info(ui, "PIDs", &number(c.stats.as_ref().and_then(|s| s.pids)));
                                info(ui, "Root filesystem size", &size(c.root_size));
                                info(ui, "Networks", &c.networks);
                                info(ui, "Ports", &c.ports);
                                info(ui, "Mounts", &c.mounts);
                            });
                        });
                    } else if let Some(c) = members.first() {
                        Grid::new("project-information").num_columns(2).show(ui, |ui| {
                            info(ui, "Compose directory", &c.workdir);
                            info(ui, "Containers", &members.len().to_string());
                            info(ui, "Running containers", &members.iter().filter(|c| c.active()).count().to_string());
                        });
                    }
                }
                if !matches!(target, Target::Container(_)) {
                    ui.label(RichText::new(tr("Containers")).strong().size(18.0));
                    let mut rows: Vec<_> =
                        members.iter().filter(|c| container_matches(c, &search.to_lowercase())).map(|c| Row::container(c, false)).collect();
                    sort_rows(&mut rows, self.sort, self.descending);
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 240.0), Layout::top_down(Align::Min), |ui| {
                        self.table(ui, rows, false, settings);
                    });
                }
                if !matches!(target, Target::Image(_)) {
                    ui.label(RichText::new(tr("Recent logs")).strong().size(18.0));
                    ui.label(
                        RichText::new(tr("Last 40 lines per container · up to 8 containers · refreshes with inventory"))
                            .small()
                            .color(muted(ui.ctx())),
                    );
                    Frame::NONE.fill(card(ui.ctx())).corner_radius(5.0).inner_margin(16.0).show(ui, |ui| {
                        ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                        ScrollArea::both().id_salt("docker-logs").max_height(240.0).auto_shrink([false, false]).show(ui, |ui| {
                            let logs = if self.logs_target.as_ref() != Some(&target) {
                                tr("Reading logs…")
                            } else if !self.fresh {
                                tr("Logs are stale until refresh succeeds.")
                            } else if snapshot.logs.trim().is_empty() {
                                tr("No logs available.")
                            } else {
                                &snapshot.logs
                            };
                            ui.add(Label::new(RichText::new(logs).monospace().size(12.0)).selectable(true).extend());
                        });
                    });
                }
            });
        });
    }
}
fn numeric_column(images: bool, index: usize) -> bool {
    if images {
        (2..=6).contains(&index)
    } else {
        matches!(index, 2..=5 | 7..=9)
    }
}
fn active_sum(members: &[&Container], counter: impl Fn(&docker::Stats) -> Option<u64>) -> Option<u64> {
    sum_sizes(members.iter().filter(|c| c.active()).map(|c| c.stats.as_ref().and_then(&counter)))
}
fn state_label(status: &str) -> &str {
    match status {
        "running" => "Running",
        "exited" => "Stopped",
        _ => status,
    }
}
fn container_matches(c: &Container, query: &str) -> bool {
    query.is_empty() || [&c.name, &c.image_name, &c.project, &c.id, &c.status, &c.service].iter().any(|s| s.to_lowercase().contains(query))
}
fn size(n: Option<u64>) -> String {
    n.map(bytes).unwrap_or_else(|| "—".into())
}
fn number(n: Option<u64>) -> String {
    n.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
}
fn percent(n: Option<f32>) -> String {
    n.map(|n| format!("{n:.1}%")).unwrap_or_else(|| "—".into())
}
fn sum_sizes(mut values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    values.try_fold(0_u64, |total, n| Some(total.saturating_add(n?)))
}
fn totals(members: &[&Container]) -> (Option<f32>, Option<u64>, Option<u64>) {
    if members.is_empty() {
        return (None, None, None);
    }
    // Stopped containers contribute zero. Missing telemetry of any active member remains unavailable.
    let active: Vec<_> = members.iter().filter(|c| c.active()).collect();
    let cpu = active.iter().try_fold(0.0, |sum, c| Some(sum + c.stats.as_ref()?.cpu?));
    let memory = sum_sizes(active.iter().map(|c| c.stats.as_ref().and_then(|s| s.memory)));
    let io = sum_sizes(active.iter().map(|c| c.stats.as_ref().and_then(|s| Some(s.read?.saturating_add(s.write?)))));
    (cpu, memory, io)
}
fn info(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(tr(label)).color(muted(ui.ctx())));
    ui.add(Label::new(if value.is_empty() || value == "null" { tr("Not reported") } else { tr(value) }).wrap());
    ui.end_row();
}
struct Row {
    target: Target,
    cells: Vec<String>,
    values: Vec<Option<f64>>,
    project: bool,
    child: bool,
    folded: bool,
    active: bool,
}
impl Row {
    fn container(c: &Container, child: bool) -> Self {
        let stats = c.stats.as_ref();
        let cpu = stats.and_then(|s| s.cpu);
        let memory = stats.and_then(|s| s.memory);
        let read = stats.and_then(|s| s.read);
        let write = stats.and_then(|s| s.write);
        let pids = stats.and_then(|s| s.pids);
        Self {
            target: Target::Container(c.id.clone()),
            cells: vec![
                c.name.clone(),
                tr(state_label(&c.status)).into(),
                percent(cpu),
                size(memory),
                format!("{} / {}", size(read), size(write)),
                size(c.size),
                c.image_name.clone(),
                format!("{} / {}", size(stats.and_then(|s| s.received)), size(stats.and_then(|s| s.sent))),
                number(pids),
                size(c.root_size),
            ],
            values: vec![
                None,
                None,
                cpu.map(f64::from),
                memory.map(|n| n as f64),
                read.zip(write).map(|(r, w)| r.saturating_add(w) as f64),
                c.size.map(|n| n as f64),
                None,
                stats.and_then(|s| s.received.zip(s.sent)).map(|(r, w)| r.saturating_add(w) as f64),
                pids.map(|n| n as f64),
                c.root_size.map(|n| n as f64),
            ],
            project: false,
            child,
            folded: false,
            active: c.active(),
        }
    }
    fn project(name: &str, members: &[&Container], folded: bool) -> Self {
        let (cpu, memory, _) = totals(members);
        let running = members.iter().filter(|c| c.active()).count();
        Self {
            target: Target::Project(name.into()),
            cells: vec![
                format!("{} ({})", if name.is_empty() { tr("Standalone containers") } else { name }, members.len()),
                format!("{running}/{} {}", members.len(), tr("running")),
                percent(cpu),
                size(memory),
                format!("{} / {}", size(active_sum(members, |s| s.read)), size(active_sum(members, |s| s.write))),
                size(sum_sizes(members.iter().map(|c| c.size))),
                "—".into(),
                format!("{} / {}", size(active_sum(members, |s| s.received)), size(active_sum(members, |s| s.sent))),
                number(active_sum(members, |s| s.pids)),
                size(sum_sizes(members.iter().map(|c| c.root_size))),
            ],
            values: vec![
                None,
                None,
                cpu.map(f64::from),
                memory.map(|n| n as f64),
                active_sum(members, |s| s.read.zip(s.write).map(|(r, w)| r.saturating_add(w))).map(|n| n as f64),
                sum_sizes(members.iter().map(|c| c.size)).map(|n| n as f64),
                None,
                active_sum(members, |s| s.received.zip(s.sent).map(|(r, w)| r.saturating_add(w))).map(|n| n as f64),
                active_sum(members, |s| s.pids).map(|n| n as f64),
                sum_sizes(members.iter().map(|c| c.root_size)).map(|n| n as f64),
            ],
            project: true,
            child: false,
            folded,
            active: running > 0,
        }
    }
    fn image(i: &docker::Image, data: &DockerSnapshot) -> Self {
        let members = data.members(&Target::Image(i.id.clone()));
        let active = members.iter().filter(|c| c.active()).count();
        let count = members.len();
        let dependents = data.images.iter().filter(|other| other.derived_from(i)).count();
        Self {
            target: Target::Image(i.id.clone()),
            cells: vec![
                i.name().into(),
                tr(if members.is_empty() { "Unused" } else { "In use" }).into(),
                size(i.size),
                active.to_string(),
                count.to_string(),
                dependents.to_string(),
                i.layers.len().to_string(),
                i.platform.clone(),
            ],
            values: vec![
                None,
                None,
                i.size.map(|n| n as f64),
                Some(active as f64),
                Some(count as f64),
                Some(dependents as f64),
                Some(i.layers.len() as f64),
                None,
            ],
            project: false,
            child: false,
            folded: false,
            active: !members.is_empty(),
        }
    }
}
fn project_rows(data: &DockerSnapshot, query: &str, folded: &HashSet<String>, column: usize, descending: bool) -> Vec<Row> {
    let groups = data.projects();
    let mut projects: Vec<_> = groups
        .iter()
        .filter(|(name, members)| query.is_empty() || name.to_lowercase().contains(query) || members.iter().any(|c| container_matches(c, query)))
        .map(|(name, members)| Row::project(name, members, folded.contains(name)))
        .collect();
    sort_rows(&mut projects, column, descending);
    let mut rows = Vec::new();
    for project in projects {
        let Target::Project(name) = &project.target else { unreachable!() };
        let mut children: Vec<_> =
            if !project.folded || !query.is_empty() { groups[name].iter().map(|c| Row::container(c, true)).collect() } else { Vec::new() };
        sort_rows(&mut children, column, descending);
        rows.push(project);
        rows.extend(children);
    }
    rows
}
fn sort_rows(rows: &mut [Row], column: usize, descending: bool) {
    rows.sort_by(|a, b| {
        let order = match (a.values.get(column).copied().flatten(), b.values.get(column).copied().flatten()) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            _ => a.cells.get(column).cmp(&b.cells.get(column)),
        };
        if descending {
            order.reverse()
        } else {
            order
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_sort_preserves_hierarchy_numeric_totals_and_missing_values() {
        let mut data = docker::fixture();
        data.containers[4].stats.as_mut().unwrap().cpu = Some(12.0);
        for column in [2, 3, 4, 5, 7, 8, 9] {
            for descending in [false, true] {
                let rows = project_rows(&data, "", &HashSet::new(), column, descending);
                let parents: Vec<_> = rows.iter().filter(|r| r.project).collect();
                assert!(parents.iter().all(|r| r.values[column].is_some()), "project metrics must be numeric");
                assert!(parents.windows(2).all(|pair| {
                    let order = pair[0].values[column].partial_cmp(&pair[1].values[column]).unwrap();
                    if descending {
                        !order.is_lt()
                    } else {
                        !order.is_gt()
                    }
                }));
                let mut parent = None;
                let mut previous: Option<&Row> = None;
                for row in &rows {
                    if let Target::Project(name) = &row.target {
                        parent = Some(name);
                        previous = None;
                    } else {
                        let c = data.containers.iter().find(|c| row.target == Target::Container(c.id.clone())).unwrap();
                        assert_eq!(Some(&c.project), parent, "containers must remain under their own project");
                        assert!(row.child);
                        if let Some(previous) = previous {
                            match (previous.values[column], row.values[column]) {
                                (Some(a), Some(b)) => assert!(if descending { a >= b } else { a <= b }),
                                (None, Some(_)) => panic!("unavailable values must stay last"),
                                _ => {}
                            }
                        }
                        previous = Some(row);
                    }
                }
                assert_eq!(rows.iter().filter(|r| r.child).count(), data.containers.len());
            }
        }
        let rows = project_rows(&data, "", &HashSet::new(), 2, true);
        assert_eq!(rows[0].target, Target::Project("analytics".into()), "12% sorts above 6.9%, regardless of text");
        data.containers[4].stats = None;
        for descending in [false, true] {
            let rows = project_rows(&data, "", &HashSet::new(), 2, descending);
            assert_eq!(rows.iter().rfind(|r| r.project).unwrap().target, Target::Project("analytics".into()));
        }
    }
    #[test]
    fn sorted_projects_keep_folding_search_and_complete_totals() {
        let data = docker::fixture();
        let folded = HashSet::from(["workspace".into()]);
        let rows = project_rows(&data, "", &folded, 0, true);
        assert_eq!(rows[0].target, Target::Project("workspace".into()));
        assert!(rows[0].folded);
        assert!(!rows.iter().any(|r| r.cells[0] == "workspace-api-1"));
        let rows = project_rows(&data, "workspace-web", &folded, 2, true);
        assert_eq!(rows.len(), 5, "search reveals the matching folded group");
        assert_eq!(rows[0].values[2], Row::project("workspace", &data.members(&Target::Project("workspace".into())), false).values[2]);
        assert_eq!(rows[1].cells[0], "workspace-api-1", "revealed children respect the selected metric");
        assert!(project_rows(&data, "no-such-container", &folded, 0, false).is_empty());
    }
    #[test]
    fn deletion_captures_full_ids_and_respects_selection_and_freshness() {
        let mut view = DockerView { data: Arc::new(docker::fixture()), fresh: true, ..Default::default() };
        assert!(view.pending_action(Action::RemoveContainer).is_none());
        view.selected = Some(Target::Project("workspace".into()));
        let pending = view.pending_action(Action::RemoveContainer).unwrap();
        assert_eq!(pending.ids.len(), 4);
        assert!(pending.active);
        assert!(!pending.force, "force deletion must be explicitly enabled");
        assert!(pending.ids.iter().all(|id| docker::valid_id(id)));
        let captured = pending.ids.clone();
        view.selected = Some(Target::Container(view.data.containers[6].id.clone()));
        let stopped = view.pending_action(Action::RemoveContainer).unwrap();
        assert!(!stopped.active);
        assert_eq!(stopped.ids.len(), 1);
        assert_ne!(captured, stopped.ids, "changing selection does not retarget an existing confirmation");
        view.selected = Some(Target::Image(view.data.images[5].id.clone()));
        assert_eq!(view.delete_action(), Action::RemoveImage);
        assert!(view.pending_action(Action::RemoveContainer).is_none());
        let image = view.pending_action(Action::RemoveImage).unwrap();
        assert_eq!(image.ids, [view.data.images[5].id.clone()]);
        assert!(image.names[0].contains(&image.ids[0]), "confirmation shows the immutable ID");
        assert!(view.can_action(Action::RemoveImage));
        view.pending = Some(image);
        assert!(!view.can_action(Action::RemoveImage), "an open dialog cannot be overwritten");
        view.pending = None;
        view.fresh = false;
        assert!(!view.can_action(Action::RemoveImage));
        view.ask_delete();
        assert!(view.pending.is_none());
    }
    #[test]
    fn refresh_clears_deleted_detail_and_refreshes_after_partial_failure() {
        let ctx = Context::default();
        let data = docker::fixture();
        let target = Target::Container(data.containers[0].id.clone());
        let mut view = DockerView { data: Arc::new(data), selected: Some(target.clone()), detail: Some(target), fresh: true, ..Default::default() };
        view.record_history();
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(Err("One member could not be removed".into())).unwrap();
        view.operation = Some(rx);
        view.last_poll = Some(Instant::now());
        assert!(view.poll(&ctx, false, 0, false).unwrap().is_err());
        assert!(!view.fresh);
        assert!(view.last_poll.is_none(), "partial deletion errors still request an inventory refresh");
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send((view.detail.clone(), Ok(DockerSnapshot::default()))).unwrap();
        view.collection = Some(rx);
        view.poll(&ctx, false, 0, false);
        assert!(view.selected.is_none());
        assert!(view.detail.is_none());
        assert!(view.cpu.is_empty());
        assert!(view.fresh);
        view.selected = Some(Target::Image("sha256:missing".into()));
        view.detail = view.selected.clone();
        view.reconcile_targets();
        assert!(view.selected.is_none());
        assert!(view.detail.is_none());
    }
    #[test]
    fn running_deletion_dialog_requires_opt_in_and_can_be_cancelled() {
        let ctx = Context::default();
        let mut view = DockerView { data: Arc::new(docker::fixture()), fresh: true, ..Default::default() };
        view.selected = Some(Target::Project("workspace".into()));
        view.pending = view.pending_action(Action::RemoveContainer);
        fn render(view: &mut DockerView, ctx: &Context, events: Vec<Event>) -> FullOutput {
            ctx.run(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))), events, ..Default::default() }, |ctx| {
                view.dialog(ctx)
            })
        }
        fn text_at(output: &FullOutput, label: &str) -> Pos2 {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    Shape::Text(t) if t.galley.text() == label => Some(t.pos + vec2(4.0, 7.0)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing dialog text: {label}"))
        }
        fn click(view: &mut DockerView, ctx: &Context, pos: Pos2) -> FullOutput {
            render(
                view,
                ctx,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
                ],
            );
            render(view, ctx, vec![Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
            render(view, ctx, vec![])
        }
        render(&mut view, &ctx, vec![]);
        let output = render(&mut view, &ctx, vec![]);
        click(&mut view, &ctx, text_at(&output, tr("Delete")));
        assert!(view.pending.is_some(), "delete is disabled until force is explicitly selected");
        assert!(view.operation.is_none());
        let output = click(&mut view, &ctx, text_at(&output, tr("Force delete running containers")));
        assert!(view.pending.as_ref().unwrap().force);
        click(&mut view, &ctx, text_at(&output, tr("Cancel")));
        assert!(view.pending.is_none());
        assert!(view.operation.is_none(), "cancel never starts a system operation");
    }
    #[test]
    fn missing_active_telemetry_does_not_become_zero() {
        let mut data = docker::fixture();
        assert!(totals(&data.members(&Target::Project("workspace".into()))).0.unwrap() > 0.0);
        data.containers[0].stats = None;
        assert_eq!(totals(&data.members(&Target::Project("workspace".into()))), (None, None, None));
        assert_eq!(totals(&[&data.containers[6]]), (Some(0.0), Some(0), Some(0)));
    }
    #[test]
    fn summaries_count_unique_containers_and_preserve_missing_sizes() {
        let mut data = docker::fixture();
        let members: Vec<_> = data.containers.iter().collect();
        let summary = Row::project("", &members, false);
        assert_eq!(summary.cells[2], "9.0%");
        assert_eq!(summary.cells[5], size(Some(28 * 1_048_576)), "writable sizes include stopped containers");
        assert_eq!(summary.cells[8], "50");
        assert_eq!(summary.cells[9], size(sum_sizes(members.iter().map(|c| c.root_size))));
        let mut rows: Vec<_> = data.containers.iter().map(|c| Row::container(c, false)).collect();
        sort_rows(&mut rows, 9, true);
        assert!(rows.windows(2).all(|pair| pair[0].values[9] >= pair[1].values[9]));
        data.containers[6].root_size = None;
        data.containers[6].size = None;
        data.containers[0].stats.as_mut().unwrap().cpu = None;
        let summary = Row::project("", &data.containers.iter().collect::<Vec<_>>(), false);
        assert_eq!(summary.cells[2], "—");
        assert_ne!(summary.cells[3], "—", "missing CPU does not conceal known memory");
        assert_eq!(summary.cells[5], "—");
        assert_eq!(summary.cells[9], "—");
    }
    #[test]
    fn container_sort_uses_numbers_and_history_is_bounded() {
        let data = docker::fixture();
        let mut rows: Vec<_> = data.containers.iter().map(|c| Row::container(c, false)).collect();
        sort_rows(&mut rows, 2, true);
        assert_eq!(rows[0].cells[0], "workspace-api-1");
        let mut view = DockerView { data: Arc::new(data), detail: Some(Target::Project("workspace".into())), ..Default::default() };
        for _ in 0..200 {
            view.record_history();
        }
        assert_eq!(view.cpu.len(), HISTORY);
        assert_eq!(view.memory.len(), HISTORY);
    }
}
