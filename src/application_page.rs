use super::*;
use crate::app_details::{self, Metadata};

pub(crate) enum UpdateState {
    Idle,
    Checking,
    Current,
    Available(crate::updates::Available),
    Updating,
    Error(String),
}
pub(crate) struct ApplicationView {
    pub process: Process,
    pub previous_search: String,
    pub metadata: Option<Metadata>,
    pub update: UpdateState,
    pub rows: Vec<(usize, usize)>,
    pub cpu: f32,
    pub memory: u64,
    pub io: Option<f32>,
    pub threads: u64,
    pub elapsed: u64,
    pub running: bool,
    pub cpu_history: Series,
    pub memory_history: Series,
    pub io_history: Series,
    pub sample: u64,
}
impl ApplicationView {
    pub fn new(process: Process, previous_search: String) -> Self {
        Self {
            process,
            previous_search,
            metadata: None,
            update: UpdateState::Idle,
            rows: vec![],
            cpu: 0.0,
            memory: 0,
            io: None,
            threads: 0,
            elapsed: 0,
            running: true,
            cpu_history: Series::new(),
            memory_history: Series::new(),
            io_history: Series::new(),
            sample: u64::MAX,
        }
    }
    pub fn key(&self) -> (u32, u64) {
        (self.process.pid, self.process.start)
    }
    pub fn refresh(&mut self, data: &Snapshot, low_memory: bool) {
        if self.sample == data.sampled {
            return;
        }
        self.sample = data.sampled;
        self.rows = app_details::descendants(&data.processes, self.key());
        self.running = !self.rows.is_empty();
        self.cpu = 0.0;
        self.memory = 0;
        self.threads = 0;
        self.io = self.running.then_some(0.0);
        for &(index, _) in &self.rows {
            let p = &data.processes[index];
            self.cpu += p.cpu;
            self.memory = self.memory.saturating_add(p.memory);
            self.threads += p.threads as u64;
            self.io = self.io.filter(|_| p.io_available).map(|io| io + p.read_rate + p.write_rate);
        }
        if let Some(&(index, _)) = self.rows.first() {
            self.process = data.processes[index].clone();
            // sysconf reads a process-wide constant; no pointer or mutable state
            // crosses this libc boundary. Linux start times are clock ticks.
            let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
            self.elapsed = data.uptime.saturating_sub(self.process.start / ticks);
        }
        if !low_memory {
            push(&mut self.cpu_history, self.cpu);
            push(&mut self.memory_history, self.memory as f32);
            if let Some(io) = self.io {
                push(&mut self.io_history, io);
            } else {
                self.io_history.clear();
            }
        }
    }
}
fn duration(seconds: u64) -> String {
    format!("{}:{:02}:{:02}:{:02}", seconds / 86400, seconds / 3600 % 24, seconds / 60 % 60, seconds % 60)
}
fn info(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(tr(label)).color(muted(ui.ctx())));
    ui.add(Label::new(if value.is_empty() { tr("Not reported") } else { tr(value) }).wrap());
    ui.end_row();
}
impl TaskManager {
    pub(crate) fn open_application(&mut self, process: Process) {
        let previous_search = self.application.as_ref().map(|a| a.previous_search.clone()).unwrap_or_else(|| self.search.clone());
        self.selected = Some((process.pid, process.start));
        let mut view = ApplicationView::new(process, previous_search);
        view.refresh(&self.data, self.settings.low_memory);
        if demo::enabled() {
            view.metadata = Some(app_details::demo(&view.process));
            if !self.settings.low_memory {
                [view.cpu_history, view.memory_history, view.io_history] = demo::application_histories(view.cpu, view.memory, view.io.unwrap_or(0.0));
            }
        }
        self.application = Some(view);
        self.search.clear();
    }
    pub(crate) fn close_application(&mut self) {
        if let Some(view) = self.application.take() {
            self.selected = Some(view.key());
            self.search = view.previous_search;
        }
        self.properties = None;
    }
    pub(crate) fn load_application_metadata(&mut self, ctx: &Context) {
        if self.metadata_busy || demo::enabled() {
            return;
        }
        let Some(view) = self.application.as_ref().filter(|a| a.metadata.is_none()) else { return };
        let process = view.process.clone();
        self.metadata_busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let metadata = app_details::inspect(&process);
            let _ = tx.send(Worker::Application((process.pid, process.start), metadata));
            ctx.request_repaint();
        });
    }
    pub(crate) fn application_toolbar(&mut self, ui: &mut Ui) {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, line(ui.ctx())));
        let mut bar = ui.new_child(
            UiBuilder::new().id_salt("application-toolbar").max_rect(r.shrink2(vec2(16.0, 8.0))).layout(Layout::left_to_right(Align::Center)),
        );
        if bar.add(quiet_button(tr("<  Back"))).clicked() {
            self.close_application();
            return;
        }
        bar.label(RichText::new(tr("Application details")).strong());
        let Some(view) = self.application.as_ref() else { return };
        let p = view.process.clone();
        let running = view.running;
        bar.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    running && !demo::enabled(),
                    Button::new(RichText::new(tr("End task")).strong().color(Color32::from_gray(24))).fill(ACCENT).min_size(vec2(120.0, 34.0)),
                )
                .clicked()
            {
                self.selected = Some((p.pid, p.start));
                self.ask_end();
            }
            if ui.add(quiet_button(tr("Properties"))).clicked() {
                self.properties = Some(p.clone());
            }
            if demo::enabled() {
                ui.label(RichText::new(tr("Demo · fictional data")).small().color(muted(ui.ctx())));
            }
        });
    }
    pub(crate) fn application_page(&mut self, ui: &mut Ui) {
        let Some(mut view) = self.application.take() else { return };
        let mut open_child = None;
        let mut child_end = None;
        let mut child_properties = None;
        let mut check_updates = false;
        let mut install_update = None;
        ui.spacing_mut().item_spacing.y = 12.0;
        ScrollArea::vertical().id_salt(("application-page", view.key())).auto_shrink([false, false]).show(ui, |ui| {
            Frame::NONE.inner_margin(24.0).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().item_spacing = vec2(16.0, 12.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::hover());
                    if let Some(app) = &view.process.desktop {
                        if let Some(image) = &app.icon {
                            let texture = self
                                .app_icons
                                .entry(app.id.clone())
                                .or_insert_with(|| ui.ctx().load_texture(&app.id, image.as_ref().clone(), TextureOptions::LINEAR));
                            ui.painter().image(texture.id(), r.shrink(4.0), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                        } else {
                            icon(ui.painter(), r.center(), 0, BLUE);
                        }
                    } else {
                        icon(ui.painter(), r.center(), 0, BLUE);
                    }
                    ui.vertical(|ui| {
                        ui.heading(RichText::new(&view.process.name).size(28.0));
                        ui.label(
                            RichText::new(format!(
                                "PID {}  ·  {}  ·  {}",
                                view.process.pid,
                                view.process.user,
                                tr(if view.running { &view.process.status } else { "Exited" })
                            ))
                            .color(muted(ui.ctx())),
                        );
                    });
                });
                if !view.running {
                    ui.colored_label(ACCENT, tr("This process has exited. Its last details are kept until you go back."));
                }
                ui.label(RichText::new(tr("Resource usage · includes all subprocesses")).color(muted(ui.ctx())));
                ui.columns(4, |columns| {
                    metric(&mut columns[0], "CPU", format!("{:.1}%", view.cpu));
                    metric(&mut columns[1], "Total memory", bytes(view.memory));
                    metric(&mut columns[2], "Disk", view.io.map(rate).unwrap_or_else(|| "—".into()));
                    metric(&mut columns[3], "Running time", duration(view.elapsed));
                });
                ui.add_space(4.0);
                if self.settings.low_memory {
                    ui.label(RichText::new(tr("Graphs are disabled in low memory mode.")).color(muted(ui.ctx())));
                } else {
                    ui.columns(3, |columns| {
                        for (i, label, series, max, color, value) in [
                            (0, "CPU", &view.cpu_history, 100.0, CYAN, format!("{:.1}%", view.cpu)),
                            (
                                1,
                                "Total memory",
                                &view.memory_history,
                                view.memory_history.iter().copied().fold(1.0, f32::max) * 1.2,
                                BLUE,
                                bytes(view.memory),
                            ),
                            (
                                2,
                                "Disk",
                                &view.io_history,
                                view.io_history.iter().copied().fold(1024.0, f32::max) * 1.2,
                                GREEN,
                                view.io.map(rate).unwrap_or_else(|| "—".into()),
                            ),
                        ] {
                            let ui = &mut columns[i];
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(tr(label)).color(color));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(value);
                                });
                            });
                            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 110.0), Sense::hover());
                            graph(ui, r, series, max, color, true, true);
                            if series.is_empty() {
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, tr("Not reported"), FontId::proportional(13.0), muted(ui.ctx()));
                            }
                        }
                    });
                    ui.label(RichText::new(tr("Last 60 samples · history starts when this page opens")).small().color(muted(ui.ctx())));
                }
                ui.add_space(4.0);
                ui.columns(2, |columns| {
                    for (i, heading) in [(0, "Process information"), (1, "Installation")] {
                        let ui = &mut columns[i];
                        Frame::NONE.fill(card(ui.ctx())).corner_radius(5.0).inner_margin(16.0).show(ui, |ui| {
                            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                            ui.label(RichText::new(tr(heading)).strong().size(16.0));
                            ui.add_space(8.0);
                            Grid::new(("application-info", i)).num_columns(2).spacing(vec2(18.0, 10.0)).show(ui, |ui| {
                                if i == 0 {
                                    info(ui, "Owner", &format!("{} (UID {})", view.process.user, view.process.uid));
                                    ui.label(RichText::new(tr("Parent process")).color(muted(ui.ctx())));
                                    if let Some(parent) = self.data.processes.iter().find(|p| {
                                        view.running && p.pid == view.process.parent && p.pid != view.process.pid && p.start <= view.process.start
                                    }) {
                                        if ui
                                            .link(RichText::new(format!("{} (PID {}) >", parent.name, parent.pid)).color(BLUE).underline())
                                            .on_hover_text(tr("Open parent process"))
                                            .clicked()
                                        {
                                            open_child = Some(parent.clone());
                                        }
                                    } else {
                                        ui.label(format!("PID {} · {}", view.process.parent, tr("Not reported")));
                                    }
                                    ui.end_row();
                                    info(ui, "Threads", &view.threads.to_string());
                                    info(ui, "Priority", &format!("Nice {}", view.process.nice));
                                    info(ui, "Memory", &bytes(if view.running { view.process.memory } else { 0 }));
                                } else if let Some(metadata) = &view.metadata {
                                    info(ui, "Installed via", &metadata.format);
                                    info(ui, "Package", &metadata.package);
                                    info(ui, "Installed version", &metadata.version);
                                } else {
                                    ui.spinner();
                                    ui.label(tr("Reading package information…"));
                                    ui.end_row();
                                }
                            });
                            if i == 1 {
                                ui.add_space(8.0);
                                match &view.update {
                                    UpdateState::Checking => {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label(tr("Checking for updates…"));
                                        });
                                    }
                                    UpdateState::Updating => {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label(tr("Updating…"));
                                        });
                                    }
                                    UpdateState::Current => {
                                        ui.label(tr("No updates found in the available package sources."));
                                    }
                                    UpdateState::Available(available) => {
                                        ui.label(format!("{}: {}", tr("Available version"), tr(&available.version)));
                                        if available.target.format == "pacman" {
                                            ui.label(tr("Arch Linux requires upgrading all system packages together."));
                                        }
                                        if ui
                                            .add_enabled(
                                                !demo::enabled() && !self.busy,
                                                Button::new(
                                                    RichText::new(tr(if available.target.format == "pacman" {
                                                        "Update all system packages"
                                                    } else {
                                                        "Update"
                                                    }))
                                                    .color(Color32::from_gray(24)),
                                                )
                                                .fill(ACCENT),
                                            )
                                            .clicked()
                                        {
                                            install_update = Some(available.target.clone());
                                        }
                                    }
                                    UpdateState::Error(error) => {
                                        ui.add(Label::new(RichText::new(tr(error)).color(ACCENT)).wrap());
                                    }
                                    UpdateState::Idle => {}
                                }
                                if ui
                                    .add_enabled(
                                        view.metadata.is_some()
                                            && !demo::enabled()
                                            && !self.busy
                                            && !matches!(view.update, UpdateState::Checking | UpdateState::Updating),
                                        Button::new(tr("Check for updates")),
                                    )
                                    .clicked()
                                {
                                    check_updates = true;
                                }
                                if view.metadata.as_ref().is_some_and(|m| m.format == "deb" || m.format == "pacman") {
                                    ui.label(
                                        RichText::new(tr(
                                            "Uses cached package sources. Refresh them in your software manager for the latest results.",
                                        ))
                                        .small()
                                        .color(muted(ui.ctx())),
                                    );
                                }
                                if let Some(metadata) = &view.metadata {
                                    if !metadata.note.is_empty() {
                                        ui.label(RichText::new(tr(&metadata.note)).small().color(muted(ui.ctx())));
                                    }
                                }
                            }
                        });
                    }
                });
                CollapsingHeader::new(tr("Executable and command line")).id_salt("application-command").show(ui, |ui| {
                    if let Some(metadata) = &view.metadata {
                        ui.add(Label::new(&metadata.executable).wrap());
                    }
                    ui.add(Label::new(&view.process.command).wrap());
                    if ui.button(tr("Copy command line")).clicked() {
                        ui.ctx().copy_text(view.process.command.clone());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} ({})", tr("Subprocesses"), view.rows.len().saturating_sub(1))).strong().size(17.0));
                    ui.label(RichText::new(tr("Double-click to open details")).small().color(muted(ui.ctx())));
                });
                let query = self.search.to_lowercase();
                let rows: Vec<_> = view
                    .rows
                    .iter()
                    .skip(1)
                    .filter(|&&(i, _)| {
                        let p = &self.data.processes[i];
                        query.is_empty()
                            || p.name.to_lowercase().contains(&query)
                            || p.pid.to_string().contains(&query)
                            || p.user.to_lowercase().contains(&query)
                    })
                    .collect();
                if rows.is_empty() {
                    ui.label(tr(if query.is_empty() { "No subprocesses" } else { "No matching processes" }));
                } else {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.push_id(view.key(), |ui| {
                        let resize = self.simple_table(
                            ui,
                            "subprocesses",
                            &[("Name", 0.35), ("PID", 0.1), ("Status", 0.15), ("CPU", 0.12), ("Memory", 0.15), ("User", 0.13)],
                            rows.len(),
                            |ui, row, r, cells| {
                                let &(index, depth) = rows[row];
                                let p = &self.data.processes[index];
                                let response = ui.interact(r, ui.id().with(("subprocess", p.pid, p.start)), Sense::click());
                                if response.hovered() {
                                    ui.painter().rect_filled(r, 0.0, card(ui.ctx()));
                                }
                                cell(
                                    ui,
                                    cells[0].shrink2(vec2(4.0, 0.0)),
                                    format!("{}{}", "  ".repeat(depth.min(8)), p.name),
                                    false,
                                    fg(ui.ctx()),
                                    14.0,
                                );
                                for (i, value) in [
                                    (1, p.pid.to_string()),
                                    (2, tr(&p.status).into()),
                                    (3, format!("{:.1}%", p.cpu)),
                                    (4, bytes(p.memory)),
                                    (5, p.user.clone()),
                                ] {
                                    cell(ui, cells[i], value, false, fg(ui.ctx()), 14.0);
                                }
                                if response.double_clicked() {
                                    open_child = Some(p.clone());
                                }
                                response.context_menu(|ui| {
                                    if ui.button(tr("Application details")).clicked() {
                                        open_child = Some(p.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button(tr("Properties")).clicked() {
                                        child_properties = Some(p.clone());
                                        ui.close_menu();
                                    }
                                    if ui.add_enabled(!demo::enabled(), Button::new(tr("End task…"))).clicked() {
                                        child_end = Some((p.pid, p.start));
                                        ui.close_menu();
                                    }
                                });
                            },
                        );
                        self.apply_column_resize(ui.ctx(), "subprocesses", resize);
                    });
                }
            });
        });
        if check_updates {
            if let Some(metadata) = view.metadata.clone() {
                view.update = UpdateState::Checking;
                let key = view.key();
                let tx = self.tx.clone();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Worker::UpdateCheck(key, crate::updates::check(&metadata)));
                    ctx.request_repaint();
                });
            }
        }
        if let Some(target) = install_update {
            if target.format == "Flatpak" && target.scope == "--user" {
                view.update = UpdateState::Updating;
                self.busy = true;
                let key = view.key();
                let tx = self.tx.clone();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Worker::Updated(key, crate::updates::install(&target)));
                    ctx.request_repaint();
                });
            } else {
                self.authorization = Some(PrivilegedAction::Update { target, key: view.key() });
            }
        }
        self.application = Some(view);
        if let Some(p) = open_child {
            self.open_application(p);
        }
        if let Some(p) = child_properties {
            self.properties = Some(p);
        }
        if let Some(key) = child_end {
            self.selected = Some(key);
            self.ask_end();
        }
    }
    pub(crate) fn application_properties(&mut self, ctx: &Context) {
        let Some(original) = self.properties.clone() else { return };
        let current = self.data.processes.iter().find(|p| (p.pid, p.start) == (original.pid, original.start));
        let process = current.cloned().unwrap_or(original);
        let running = current.is_some();
        let mut open = true;
        let mut action = None;
        Window::new(tr("Properties"))
            .id(Id::new("application-properties"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new(&process.name).strong().size(20.0));
                Grid::new("process-properties-grid").num_columns(2).spacing(vec2(20.0, 12.0)).show(ui, |ui| {
                    info(ui, "PID", &process.pid.to_string());
                    info(ui, "Owner", &format!("{} (UID {})", process.user, process.uid));
                    info(ui, "Status", if running { &process.status } else { "Exited" });
                    info(ui, "Priority", &format!("Nice {}", process.nice));
                });
                ui.separator();
                ui.label(tr("Reduce scheduling priority (nice +10)"));
                if ui.add_enabled(running && process.nice < 10 && !demo::enabled(), Button::new(tr("Reduce priority"))).clicked() {
                    action = Some(ProcessAction::Efficiency);
                }
                ui.separator();
                ui.label(tr("Suspend pauses execution; Resume continues it."));
                if ui
                    .add_enabled(running && !demo::enabled(), Button::new(tr(if process.status == "Suspended" { "Resume" } else { "Suspend" })))
                    .clicked()
                {
                    action = Some(ProcessAction::Signal(if process.status == "Suspended" { libc::SIGCONT } else { libc::SIGSTOP }));
                }
                if demo::enabled() {
                    ui.label(tr("Demo mode: system actions are disabled."));
                }
                ui.add_space(8.0);
                if ui.button(tr("Close")).clicked() {
                    self.properties = None;
                }
            });
        if !open {
            self.properties = None;
        }
        if let Some(action) = action {
            self.process_action(&process, action);
        }
    }
}
