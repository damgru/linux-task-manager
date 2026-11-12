use super::*;
fn fixture() -> TaskManager {
    let (tx, rx) = mpsc::channel();
    let process = |pid, parent, name: &str| Process { pid, parent, start: pid as u64, name: name.into(), ..Default::default() };
    TaskManager {
        application: None,
        properties: None,
        metadata_busy: false,
        network_samples: HashMap::new(),
        application_request: None,
        monitor: Monitor::idle(),
        data: Arc::new(Snapshot {
            processes: vec![process(10, 0, "parent"), process(11, 10, "child"), process(12, 11, "grandchild")],
            mem_total: 1024,
            ..Default::default()
        }),
        page: Page::Processes,
        resource: Resource::Cpu,
        collapsed: false,
        search: String::new(),
        sort: Sort::Name,
        descending: false,
        selected: None,
        kill_dialog: None,
        force: false,
        run_dialog: false,
        command: String::new(),
        message: None,
        settings: Settings::default(),
        services: vec![],
        services_error: String::new(),
        service_selected: None,
        startups: vec![],
        startup_selected: None,
        busy: false,
        tx,
        rx,
        last_services: Instant::now(),
        history: HashMap::new(),
        session: Instant::now(),
        logical: true,
        screenshot: Some(PathBuf::from("/tmp/not-written-by-ui-test.png")),
        frames: 0,
        screenshot_requested: false,
        folded: HashSet::new(),
        expanded_apps: HashSet::new(),
        authorization: None,
        app_icons: HashMap::new(),
    }
}
fn render(app: &mut TaskManager, ctx: &Context, events: Vec<Event>, settings: bool) -> FullOutput {
    ctx.run(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 1000.0))), events, ..Default::default() }, |ctx| {
        CentralPanel::default().show(ctx, |ui| {
            if settings {
                app.settings_ui(ui)
            } else {
                ui.spacing_mut().item_spacing.y = 0.0;
                app.toolbar(ui);
                if app.application.is_some() {
                    app.application_page(ui);
                } else {
                    app.process_table(ui, false);
                }
            }
        });
        app.dialogs(ctx);
    })
}
fn text_position(output: &FullOutput, label: &str) -> Option<Pos2> {
    fn find(shape: &Shape, label: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(text) if text.galley.text() == label => Some(text.pos),
            Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|s| find(&s.shape, label))
}
fn click(app: &mut TaskManager, ctx: &Context, pos: Pos2, button: PointerButton, settings: bool) -> FullOutput {
    render(app, ctx, vec![Event::PointerMoved(pos), Event::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE }], settings);
    render(app, ctx, vec![Event::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE }], settings);
    render(app, ctx, vec![], settings)
}
#[test]
fn interactive_tree_columns_language_and_authorization() {
    i18n::set_language(Language::English);
    exercise_resource_heat();
    exercise_resizing();
    exercise_footer_and_start();
    exercise_total_memory();
    exercise_total_cpu();
    exercise_application_details();
    exercise_application_updates_parent_and_network();
    exercise_separate_tabs();
    let mut app = fixture();
    let ctx = Context::default();
    render(&mut app, &ctx, vec![], false);
    let output = render(&mut app, &ctx, vec![], false);
    let name_heading = text_position(&output, "Name").unwrap();
    let toolbar_heading = text_position(&output, "Processes").unwrap();
    assert!(name_heading.y > toolbar_heading.y + 40.0, "table headings remain below the toolbar");
    assert!(text_position(&output, "Apps (0)").is_none());
    assert!(text_position(&output, "Background processes (3)").is_some());
    let parent = text_position(&output, "parent").expect("parent visible");
    click(&mut app, &ctx, parent + vec2(-40.0, 7.0), PointerButton::Primary, false);
    assert!(app.folded.contains(&(10, 10)), "tree chevron collapses the parent");
    let output = render(&mut app, &ctx, vec![], false);
    assert!(text_position(&output, "child").is_none());
    app.search = "grandchild".into();
    let output = render(&mut app, &ctx, vec![], false);
    assert!(text_position(&output, "parent").is_some());
    assert!(text_position(&output, "grandchild").is_some(), "search reveals matches in folded branches");
    app.search.clear();
    let name = text_position(&output, "Name").unwrap();
    let output = click(&mut app, &ctx, name + vec2(10.0, 5.0), PointerButton::Secondary, false);
    let parent_column = text_position(&output, "Parent PID").expect("right-click opens column menu");
    click(&mut app, &ctx, parent_column + vec2(5.0, 5.0), PointerButton::Primary, false);
    assert!(app.settings.columns.contains(&Column::Parent));
    render(
        &mut app,
        &ctx,
        vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
        false,
    );
    // Exercise egui's actual drag/drop payload and persisted non-leading Name.
    let output = render(&mut app, &ctx, vec![], false);
    let cpu = text_position(&output, "CPU").unwrap() + vec2(5.0, 6.0);
    let name = text_position(&output, "Name").unwrap() + vec2(5.0, 6.0);
    render(
        &mut app,
        &ctx,
        vec![Event::PointerMoved(cpu), Event::PointerButton { pos: cpu, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }],
        false,
    );
    render(&mut app, &ctx, vec![Event::PointerMoved(name)], false);
    render(
        &mut app,
        &ctx,
        vec![Event::PointerButton { pos: name, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }],
        false,
    );
    assert_eq!(app.settings.columns[0], Column::Cpu, "dragged CPU before Name");
    let saved = app.settings.columns.iter().map(|c| c.key()).collect::<Vec<_>>().join(",");
    assert_eq!(Column::parse_list(&saved), app.settings.columns);
    let output = render(&mut app, &ctx, vec![], true);
    let on_top = text_position(&output, "Always on top").unwrap() + vec2(5.0, 5.0);
    let press_top = |app: &mut TaskManager| {
        render(
            app,
            &ctx,
            vec![
                Event::PointerMoved(on_top),
                Event::PointerButton { pos: on_top, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
            ],
            true,
        );
        render(
            app,
            &ctx,
            vec![Event::PointerButton { pos: on_top, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }],
            true,
        )
    };
    let output = press_top(&mut app);
    assert!(app.settings.always_top);
    assert!(output.viewport_output.values().any(|v| v.commands.iter().any(|c| matches!(c, ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop)))));
    let output = press_top(&mut app);
    assert!(!app.settings.always_top);
    assert!(output.viewport_output.values().any(|v| v.commands.iter().any(|c| matches!(c, ViewportCommand::WindowLevel(WindowLevel::Normal)))));
    let output = render(&mut app, &ctx, vec![], true);
    let enabled = text_position(&output, "Enabled").unwrap();
    click(&mut app, &ctx, enabled + vec2(4.0, 5.0), PointerButton::Primary, true);
    assert!(app.settings.hide_virtual_networks, "network filter can be enabled in Settings");
    app.data = Arc::new(Snapshot {
        sampled: 1,
        networks: vec![
            Network { name: "docker0".into(), ..Default::default() },
            Network { name: "wlan0".into(), physical: true, wireless: true, ..Default::default() },
        ],
        ..Default::default()
    });
    app.resource = Resource::Network(0);
    let output = ctx.run(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 1000.0))), ..Default::default() }, |ctx| {
        CentralPanel::default().show(ctx, |ui| app.performance(ui));
    });
    assert_eq!(app.resource, Resource::Network(1), "hidden selected adapter falls back to physical adapter");
    assert!(text_position(&output, "docker0").is_none());
    assert!(text_position(&output, "Wi-Fi").is_some());
    // The extra footer card makes Settings taller than the viewport. Return
    // to its top after keyboard focus scrolled the lower controls into view.
    render(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(pos2(600.0, 300.0)),
            Event::MouseWheel { unit: MouseWheelUnit::Point, delta: vec2(0.0, 2000.0), modifiers: Modifiers::NONE },
        ],
        true,
    );
    for _ in 0..20 {
        render(&mut app, &ctx, vec![], true);
    }
    let output = render(&mut app, &ctx, vec![], true);
    let auto = text_position(&output, "Detect automatically").expect("automatic detection is the default");
    let output = click(&mut app, &ctx, auto + vec2(5.0, 5.0), PointerButton::Primary, true);
    let polish = text_position(&output, "Polski").expect("Polish language choice");
    click(&mut app, &ctx, polish + vec2(5.0, 5.0), PointerButton::Primary, true);
    assert_eq!(app.settings.language, Language::Polish);
    let output = render(&mut app, &ctx, vec![], true);
    assert!(text_position(&output, "Język").is_some(), "translation applies without restart");
    app.authorization = Some(PrivilegedAction::Process { pid: 10, start: 10, name: "parent".into(), action: ProcessAction::Signal(libc::SIGTERM) });
    render(&mut app, &ctx, vec![], true);
    let output = render(&mut app, &ctx, vec![], true);
    assert!(text_position(&output, "Wymagane uprawnienia administratora").is_some());
    let cancel = text_position(&output, "Anuluj").expect("authorization can be cancelled");
    click(&mut app, &ctx, cancel + vec2(5.0, 5.0), PointerButton::Primary, true);
    assert!(app.authorization.is_none());
    i18n::set_language(Language::English);
}

fn exercise_resource_heat() {
    let gib = 1024 * 1024 * 1024;
    let columns = [Column::Cpu, Column::TotalCpu, Column::Memory, Column::TotalMemory, Column::Disk];
    let mut process = Process {
        cpu: 0.1,
        total_cpu: 0.2,
        memory: 100 * 1024 * 1024,
        total_memory: 200 * 1024 * 1024,
        io_available: true,
        read_rate: 16_384.0,
        ..Default::default()
    };
    for dark in [true, false] {
        for column in columns {
            assert_eq!(resource_heat(column, &process, 16 * gib, dark), None, "small readings remain neutral");
            assert_eq!(resource_heat(column, &Process::default(), 16 * gib, dark), None, "zero readings remain neutral");
        }
        let boundary =
            Process { cpu: 5.0, total_cpu: 5.0, memory: gib, total_memory: gib, read_rate: 1_048_576.0, io_available: true, ..Default::default() };
        for column in columns {
            assert_eq!(resource_heat(column, &boundary, 20 * gib, dark), None, "threshold itself remains neutral");
        }
        process.cpu = 10.0;
        process.total_cpu = 15.0;
        process.memory = 2 * gib;
        process.total_memory = 3 * gib;
        process.read_rate = 2_097_152.0;
        let heat: Vec<_> = columns.iter().map(|&c| resource_heat(c, &process, 16 * gib, dark).unwrap()).collect();
        let high = Process { cpu: 90.0, total_cpu: 95.0, memory: 8 * gib, total_memory: 12 * gib, read_rate: 20_971_520.0, ..process.clone() };
        for (&column, moderate) in columns.iter().zip(&heat) {
            assert_ne!(Some(*moderate), resource_heat(column, &high, 16 * gib, dark), "higher usage gets stronger shading");
        }
        assert_eq!(resource_heat(Column::Memory, &high, 0, dark), None);
        assert_eq!(resource_heat(Column::TotalMemory, &high, 0, dark), None);
        assert_eq!(resource_heat(Column::Disk, &Process { io_available: false, ..high.clone() }, 16 * gib, dark), None);
        assert_eq!(resource_heat(Column::Cpu, &Process { cpu: f32::NAN, ..high.clone() }, 16 * gib, dark), None);
        assert_eq!(resource_heat(Column::TotalCpu, &Process { total_cpu: f32::NAN, ..high.clone() }, 16 * gib, dark), None);
        assert_eq!(resource_heat(Column::Disk, &Process { read_rate: f32::INFINITY, ..high }, 16 * gib, dark), None);

        let mut app = fixture();
        let ctx = Context::default();
        ctx.set_visuals(if dark { Visuals::dark() } else { Visuals::light() });
        let mut data = (*app.data).clone();
        data.mem_total = 16 * gib;
        data.processes = vec![Process { pid: 10, start: 10, name: "resource-test".into(), ..process.clone() }];
        app.data = Arc::new(data);
        let count_heat =
            |output: &FullOutput| output.shapes.iter().filter(|s| matches!(&s.shape, Shape::Rect(rect) if heat.contains(&rect.fill))).count();
        for page in [Page::Apps, Page::Processes] {
            app.page = page;
            Arc::make_mut(&mut app.data).processes[0].app = page == Page::Apps;
            render(&mut app, &ctx, vec![], false);
            assert_eq!(count_heat(&render(&mut app, &ctx, vec![], false)), 5, "each busy resource cell is shaded");
            app.selected = Some((10, 10));
            assert_eq!(count_heat(&render(&mut app, &ctx, vec![], false)), 0, "selection stays readable");
            app.selected = None;
            let p = &mut Arc::make_mut(&mut app.data).processes[0];
            p.cpu = 0.1;
            p.total_cpu = 0.2;
            p.memory = 100 * 1024 * 1024;
            p.total_memory = 200 * 1024 * 1024;
            p.read_rate = 16_384.0;
            assert_eq!(count_heat(&render(&mut app, &ctx, vec![], false)), 0, "idle rows have no resource shading");
            Arc::make_mut(&mut app.data).processes[0] =
                Process { app: page == Page::Apps, pid: 10, start: 10, name: "resource-test".into(), ..process.clone() };
        }
        process = Process {
            cpu: 0.1,
            total_cpu: 0.2,
            memory: 100 * 1024 * 1024,
            total_memory: 200 * 1024 * 1024,
            io_available: true,
            read_rate: 16_384.0,
            ..Default::default()
        };
    }
}

#[test]
fn existing_column_preferences_gain_total_memory_once() {
    let legacy = Settings::parse("columns=PID,Memory,Name\ndetail_columns=Name,CPU\ntree=false\n");
    assert_eq!(legacy.columns, [Column::Pid, Column::Memory, Column::TotalMemory, Column::Name, Column::TotalCpu]);
    assert_eq!(legacy.detail_columns, [Column::Name, Column::Cpu]);
    assert!(!legacy.tree);
    let hidden = Settings::parse("columns=Name,Memory\ncolumns_version=3\n");
    assert_eq!(hidden.columns, [Column::Name, Column::Memory]);
    let moved = Settings::parse("columns=Total memory,Name,Memory\ncolumns_version=3\n");
    assert_eq!(moved.columns, [Column::TotalMemory, Column::Name, Column::Memory]);
    assert_eq!(Settings::parse("").columns, Column::defaults());
}

#[test]
fn existing_column_preferences_gain_total_cpu_once() {
    let legacy = Settings::parse("columns=PID,CPU,Name,Total memory\ndetail_columns=Name,CPU\ncolumns_version=2\n");
    assert_eq!(legacy.columns, [Column::Pid, Column::Cpu, Column::TotalCpu, Column::Name, Column::TotalMemory]);
    assert_eq!(legacy.detail_columns, [Column::Name, Column::Cpu]);
    assert_eq!(Settings::parse(&legacy.serialize()).columns, legacy.columns);
    let mut hidden = legacy;
    hidden.columns.retain(|c| *c != Column::TotalCpu);
    assert_eq!(Settings::parse(&hidden.serialize()).columns, hidden.columns, "explicit hiding survives restart");
    let moved = Settings::parse("columns=Total CPU,Name,CPU\ncolumns_version=3\n");
    assert_eq!(Settings::parse(&moved.serialize()).columns, [Column::TotalCpu, Column::Name, Column::Cpu]);
}

#[test]
fn footer_preferences_and_start_page_survive_reload() {
    assert_eq!(Settings::default().start, Page::Apps);
    assert_eq!(Settings::parse("start=invalid\n").start, Page::Apps);
    let legacy = Settings::parse("start=processes\ninterval=2000\n");
    assert_eq!(legacy.start, Page::Processes, "retain an existing explicit start page");
    assert_eq!(legacy.footer, Settings::default().footer, "legacy settings retain the original footer");
    assert_eq!(Settings::parse("footer=cpu,cpu,unknown,swap\n").footer, [FooterItem::Cpu, FooterItem::Swap]);
    for page in Page::all() {
        for mask in 0..64 {
            let footer: Vec<_> = FooterItem::all().into_iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, item)| item).collect();
            let settings = Settings { start: page, footer, ..Default::default() };
            let reloaded = Settings::parse(&settings.serialize());
            assert_eq!(reloaded.start, page);
            assert_eq!(reloaded.footer, settings.footer, "every combination, including an empty footer, persists");
        }
    }
}

fn exercise_footer_and_start() {
    let mut app = fixture();
    app.data = Arc::new(Snapshot {
        load: Some([1.25, 0.84, 0.65]),
        cpu: 12.0,
        swap_total: 1024 * 1024,
        swap_free: 256 * 1024,
        gpus: vec![Gpu { busy: Some(4.0), ..Default::default() }, Gpu { busy: Some(12.0), ..Default::default() }],
        ..(*app.data).clone()
    });
    let ctx = Context::default();
    render(&mut app, &ctx, vec![], true);
    let output = render(&mut app, &ctx, vec![], true);
    let start = text_position(&output, "Apps").expect("Apps is the default start page");
    let output = click(&mut app, &ctx, start + vec2(5.0, 5.0), PointerButton::Primary, true);
    let services = text_position(&output, "Services").expect("start page choices include Services");
    click(&mut app, &ctx, services + vec2(5.0, 5.0), PointerButton::Primary, true);
    assert_eq!(app.settings.start, Page::Services);
    assert_eq!(app.page, Page::Processes, "choosing a start page does not navigate away immediately");
    let output = render(&mut app, &ctx, vec![], true);
    let footer = text_position(&output, "3 / 6 selected").expect("footer dropdown");
    let mut output = click(&mut app, &ctx, footer + vec2(5.0, 5.0), PointerButton::Primary, true);
    for item in FooterItem::all() {
        let position = text_position(&output, item.label()).expect("all six footer choices remain available");
        let was_visible = app.settings.footer.contains(&item);
        output = click(&mut app, &ctx, position + vec2(5.0, 5.0), PointerButton::Primary, true);
        assert_ne!(app.settings.footer.contains(&item), was_visible);
        assert!(text_position(&output, "Process count").is_some(), "multiple choices can be toggled without reopening");
    }
    assert!(app.settings.footer.contains(&FooterItem::Load));
    assert!(!app.settings.footer.contains(&FooterItem::Memory));
    app.settings = Settings::parse(&app.settings.serialize());
    ctx.memory_mut(|memory| memory.close_popup());
    let render_bar = |app: &mut TaskManager| {
        ctx.run(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 640.0))), ..Default::default() }, |ctx| {
            app.statusbar(ctx);
        })
    };
    render_bar(&mut app);
    let output = render_bar(&mut app);
    assert!(text_position(&output, "Load 1.25 / 0.84 / 0.65").is_some());
    assert!(text_position(&output, "GPU 12%").is_some());
    assert!(text_position(&output, "Swap 768 KB / 1.0 MB").is_some());
    assert!(text_position(&output, "3 Processes").is_none());
    assert!(text_position(&output, "CPU 12%").is_none());
    app.settings.footer = FooterItem::all().to_vec();
    render_bar(&mut app);
    let output = render_bar(&mut app);
    assert!(text_position(&output, "3 Processes").is_some());
    assert!(text_position(&output, "CPU 12%").is_some());
    let mut missing = (*app.data).clone();
    missing.load = None;
    missing.gpus = vec![Gpu::default()];
    assert_eq!(FooterItem::Load.value(&missing), "Load —");
    assert_eq!(FooterItem::Gpu.value(&missing), "GPU —");
    app.settings.footer.clear();
    render_bar(&mut app);
    let output = render_bar(&mut app);
    assert!(text_position(&output, "3 Processes").is_none());
    assert!(text_position(&output, "CPU 12%").is_none());
    assert!(text_position(&output, "Live · 1.0 s").is_some(), "update state remains visible even with no counters");
}

fn exercise_total_memory() {
    for page in [Page::Apps, Page::Processes] {
        let mut app = fixture();
        app.page = page;
        let ps = &mut Arc::make_mut(&mut app.data).processes;
        for (i, p) in ps.iter_mut().enumerate() {
            p.memory = (i as u64 + 1) * 1024 * 1024;
        }
        ps[0].app = page == Page::Apps;
        ps.push(Process { pid: 20, start: 20, name: "standalone".into(), app: page == Page::Apps, memory: 4 * 1024 * 1024, ..Default::default() });
        update_process_totals(ps);
        let ctx = Context::default();
        render(&mut app, &ctx, vec![], false);
        let output = render(&mut app, &ctx, vec![], false);
        assert!(text_position(&output, "Total memory").is_some());
        assert!(text_position(&output, &bytes(6 * 1024 * 1024)).is_some(), "parent includes child and grandchild");
        let heading = text_position(&output, "Total memory").unwrap() + vec2(5.0, 5.0);
        let output = click(&mut app, &ctx, heading, PointerButton::Primary, false);
        assert!(app.sort == Sort::TotalMemory && app.descending);
        assert!(text_position(&output, "parent").unwrap().y < text_position(&output, "standalone").unwrap().y);
        let output = click(&mut app, &ctx, heading, PointerButton::Primary, false);
        assert!(!app.descending);
        assert!(text_position(&output, "parent").unwrap().y > text_position(&output, "standalone").unwrap().y);
        app.folded.insert((10, 10));
        app.search = "parent".into();
        let output = render(&mut app, &ctx, vec![], false);
        assert!(text_position(&output, "child").is_none());
        assert!(text_position(&output, &bytes(6 * 1024 * 1024)).is_some(), "search and folds do not change totals");
        app.settings.tree = false;
        let output = render(&mut app, &ctx, vec![], false);
        assert!(text_position(&output, &bytes(6 * 1024 * 1024)).is_some(), "list view keeps subtree totals");
    }
    i18n::set_language(Language::Polish);
    assert_eq!(tr("Total memory"), "Pamięć Razem");
    i18n::set_language(Language::English);
}

fn exercise_total_cpu() {
    for page in [Page::Apps, Page::Processes] {
        let mut app = fixture();
        app.page = page;
        let data = Arc::make_mut(&mut app.data);
        data.cpu = 17.0;
        for (i, p) in data.processes.iter_mut().enumerate() {
            p.cpu = i as f32 + 1.0;
        }
        data.processes[0].app = page == Page::Apps;
        data.processes.push(Process { pid: 20, start: 20, name: "standalone".into(), app: page == Page::Apps, cpu: 4.0, ..Default::default() });
        update_process_totals(&mut data.processes);
        let ctx = Context::default();
        render(&mut app, &ctx, vec![], false);
        let output = render(&mut app, &ctx, vec![], false);
        let heading = text_position(&output, "Total CPU").unwrap() + vec2(5.0, 5.0);
        assert!(text_position(&output, "1.0%").is_some(), "own CPU remains available");
        assert!(text_position(&output, "6.0%").is_some(), "parent includes child and grandchild");
        let heading_y = text_position(&output, "Total CPU").unwrap().y;
        let header_totals =
            output.shapes.iter().filter(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "17%" && t.pos.y < heading_y)).count();
        assert_eq!(header_totals, 2, "both CPU headers show system utilization, not overlapping subtree sums");
        let output = click(&mut app, &ctx, heading, PointerButton::Primary, false);
        assert!(app.sort == Sort::TotalCpu && app.descending);
        assert!(text_position(&output, "parent").unwrap().y < text_position(&output, "standalone").unwrap().y);
        let output = click(&mut app, &ctx, heading, PointerButton::Primary, false);
        assert!(!app.descending);
        assert!(text_position(&output, "parent").unwrap().y > text_position(&output, "standalone").unwrap().y);
        app.folded.insert((10, 10));
        app.search = "parent".into();
        let output = render(&mut app, &ctx, vec![], false);
        assert!(text_position(&output, "child").is_none());
        assert!(text_position(&output, "6.0%").is_some(), "folds and search do not change totals");
        app.settings.tree = false;
        assert!(text_position(&render(&mut app, &ctx, vec![], false), "6.0%").is_some(), "list view preserves totals");
    }
    i18n::set_language(Language::Polish);
    assert_eq!(tr("Total CPU"), "CPU Razem");
    i18n::set_language(Language::English);
}

fn exercise_separate_tabs() {
    let mut app = fixture();
    let data = Arc::make_mut(&mut app.data);
    data.processes.push(Process { pid: 20, start: 20, parent: 10, name: "desktop-app".into(), app: true, ..Default::default() });
    data.processes.push(Process { pid: 21, start: 21, parent: 20, name: "app-helper".into(), ..Default::default() });
    let ctx = Context::default();
    let render = |app: &mut TaskManager, events| {
        ctx.run(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 1000.0))), events, ..Default::default() }, |ctx| {
            app.sidebar(ctx);
            CentralPanel::default().show(ctx, |ui| {
                app.toolbar(ui);
                app.process_table(ui, false);
            });
        })
    };
    render(&mut app, vec![]);
    let output = render(&mut app, vec![]);
    assert!(text_position(&output, "parent").is_some());
    assert!(text_position(&output, "desktop-app").is_none());
    let apps = text_position(&output, "Apps").expect("Apps has a navigation tab") + vec2(5.0, 5.0);
    app.selected = Some((10, 10));
    app.search = "parent".into();
    render(
        &mut app,
        vec![
            Event::PointerMoved(apps),
            Event::PointerButton { pos: apps, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ],
    );
    render(&mut app, vec![Event::PointerButton { pos: apps, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    let output = render(&mut app, vec![]);
    assert_eq!(app.page, Page::Apps);
    assert!(app.selected.is_none(), "a hidden process cannot stay selected in another tab");
    assert!(app.search.is_empty());
    assert!(text_position(&output, "Apps (1)").is_some());
    assert!(text_position(&output, "desktop-app").is_some());
    assert!(text_position(&output, "app-helper").is_none(), "applications start collapsed");
    let desktop_app = text_position(&output, "desktop-app").unwrap() + vec2(-40.0, 7.0);
    render(
        &mut app,
        vec![
            Event::PointerMoved(desktop_app),
            Event::PointerButton { pos: desktop_app, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ],
    );
    render(&mut app, vec![Event::PointerButton { pos: desktop_app, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    let output = render(&mut app, vec![]);
    assert!(text_position(&output, "app-helper").is_some(), "application chevron expands helpers");
    assert!(app.expanded_apps.contains(&(20, 20)));
    assert!(app.folded.is_empty(), "application folds do not affect background processes");
    Arc::make_mut(&mut app.data).processes.push(Process { pid: 22, start: 22, parent: 21, name: "nested-helper".into(), ..Default::default() });
    let output = render(&mut app, vec![]);
    assert!(text_position(&output, "nested-helper").is_none(), "nested application levels also start folded");
    app.search = "nested-helper".into();
    let output = render(&mut app, vec![]);
    assert!(text_position(&output, "nested-helper").is_some(), "search reveals folded application matches");
    app.search.clear();
    assert!(text_position(&output, "parent").is_none());
    assert!(text_position(&output, "Background processes (3)").is_none());
    let processes = text_position(&output, "Processes").expect("Processes has a separate tab") + vec2(5.0, 5.0);
    render(
        &mut app,
        vec![
            Event::PointerMoved(processes),
            Event::PointerButton { pos: processes, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ],
    );
    render(&mut app, vec![Event::PointerButton { pos: processes, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    let output = render(&mut app, vec![]);
    assert_eq!(app.page, Page::Processes);
    assert!(text_position(&output, "Background processes (3)").is_some());
    assert!(text_position(&output, "grandchild").is_some(), "all background levels start expanded");
    assert!(text_position(&output, "desktop-app").is_none());
    for page in Page::all() {
        let key = page.title().to_lowercase().replace(' ', "-");
        assert_eq!(Page::from(&key), page, "start page and command-line names round-trip");
    }
}

fn render_resizable_page(app: &mut TaskManager, ctx: &Context, events: Vec<Event>) -> FullOutput {
    let time = ctx.input(|i| i.time) + 0.05;
    ctx.run(
        RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 1000.0))), time: Some(time), events, ..Default::default() },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| match app.page {
                Page::Apps | Page::Processes | Page::Details => app.process_table(ui, app.page == Page::Details),
                Page::Services => app.service_table(ui),
                Page::Startup => app.startup_table(ui),
                Page::History => app.history_table(ui),
                Page::Users => app.users_table(ui),
                _ => unreachable!(),
            });
        },
    )
}

fn exercise_resizing() {
    for (page, table, first, second, row_text) in [
        (Page::Apps, "process", "Name", "Type", "App"),
        (Page::Processes, "process", "Name", "Type", "Background process"),
        (Page::Details, "details", "Name", "PID", "200"),
        (Page::Services, "services", "Name", "Description", "Demo audio service"),
        (Page::Startup, "startup", "Name", "Command", "/opt/demo/Meadow Music"),
        (Page::History, "history", "Name", "PID", "999"),
        (Page::Users, "users", "User", "Processes", "16"),
    ] {
        let mut app = fixture();
        app.settings.language = Language::English;
        app.page = page;
        app.data = Arc::new(demo::snapshot());
        app.services = demo::services();
        app.startups = demo::startups();
        app.history.insert(
            (999, 999),
            HistoryRow {
                name: "History fixture".into(),
                pid: 999,
                cpu_seconds: 2.0,
                read: 0.0,
                write: 0.0,
                last_ticks: 0,
                last_seen: Instant::now(),
            },
        );
        let ctx = Context::default();
        // Start with a known width, then exercise the actual hit targets and drawing.
        let key = format!("{table}/{first}");
        app.settings.column_widths.insert(key.clone(), 300.0);
        render_resizable_page(&mut app, &ctx, vec![]);
        let before = render_resizable_page(&mut app, &ctx, vec![]);
        let heading = text_position(&before, first).unwrap();
        let second_before = text_position(&before, second).unwrap();
        let row_before = text_position(&before, row_text).unwrap();
        let edge = pos2(heading.x - 9.0 + 300.0, heading.y + 5.0);
        let hover = render_resizable_page(&mut app, &ctx, vec![Event::PointerMoved(edge)]);
        assert_eq!(hover.platform_output.cursor_icon, CursorIcon::ResizeHorizontal, "{page:?} exposes a resize cursor");
        let order = app.settings.columns.clone();
        let sort = app.sort;
        render_resizable_page(
            &mut app,
            &ctx,
            vec![Event::PointerButton { pos: edge, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }],
        );
        for delta in [35.0, 60.0, 60.0, 80.0] {
            render_resizable_page(&mut app, &ctx, vec![Event::PointerMoved(edge + vec2(delta, 0.0))]);
            assert_eq!(app.settings.column_widths[&key], 300.0 + delta, "{page:?} tracks the full drag and holds when stationary");
        }
        let end = edge + vec2(80.0, 0.0);
        render_resizable_page(
            &mut app,
            &ctx,
            vec![Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }],
        );
        let after = render_resizable_page(&mut app, &ctx, vec![]);
        assert_eq!(app.settings.column_widths[&key], 380.0, "{page:?} retains the width on release");
        assert_eq!(app.settings.columns, order, "resizing does not reorder");
        assert!(app.sort == sort, "resizing does not sort");
        assert!((text_position(&after, second).unwrap().x - second_before.x - 80.0).abs() < 0.1, "{page:?} moves the next heading");
        assert!((text_position(&after, row_text).unwrap().x - row_before.x - 80.0).abs() < 0.1, "{page:?} keeps data aligned with headings");
        let reloaded = Settings::parse(&app.settings.serialize());
        assert_eq!(reloaded.column_widths[&key], 380.0);
        let second_ctx = Context::default();
        app.settings = reloaded;
        render_resizable_page(&mut app, &second_ctx, vec![]);
        let reloaded_output = render_resizable_page(&mut app, &second_ctx, vec![]);
        assert!(
            (text_position(&reloaded_output, second).unwrap().x - text_position(&after, second).unwrap().x).abs() < 0.1,
            "width survives a new UI session"
        );
        // Drag far left to the minimum, then release: the column remains usable.
        let edge = pos2(heading.x - 9.0 + 380.0, edge.y);
        render_resizable_page(&mut app, &second_ctx, vec![Event::PointerMoved(edge)]);
        render_resizable_page(
            &mut app,
            &second_ctx,
            vec![Event::PointerButton { pos: edge, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }],
        );
        let end = edge - vec2(350.0, 0.0);
        render_resizable_page(&mut app, &second_ctx, vec![Event::PointerMoved(end)]);
        render_resizable_page(
            &mut app,
            &second_ctx,
            vec![Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }],
        );
        assert_eq!(app.settings.column_widths[&key], 48.0);
    }
}

#[test]
fn column_width_preferences_validate_and_keep_independent_tables() {
    let settings = Settings::parse("column_widths=process/Name:321.5|details/Name:180|services/Name:400|users/CPU:NaN|history/PID:inf|startup/Name:-20|users/User:99999|process/CPU:48\n");
    assert_eq!(settings.column_widths.len(), 4);
    assert_eq!(settings.column_widths["process/Name"], 321.5);
    assert_eq!(settings.column_widths["details/Name"], 180.0);
    assert_eq!(settings.column_widths["services/Name"], 400.0);
    assert_eq!(Settings::parse(&settings.serialize()).column_widths, settings.column_widths);
    assert!(Settings::parse("").column_widths.is_empty());
}

#[test]
fn frameless_button_responds_to_hover_press_and_disabling() {
    for dark in [true, false] {
        let ctx = Context::default();
        ctx.set_visuals(if dark { Visuals::dark() } else { Visuals::light() });
        let mut time = 0.0;
        let mut frame = |events, enabled| {
            time += 0.05;
            let mut response = None;
            let output = ctx.run(
                RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 120.0))), time: Some(time), events, ..Default::default() },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        response = Some(ui.add_enabled(enabled, quiet_button("Example action")));
                    });
                },
            );
            (output, response.unwrap())
        };
        fn fill(output: &FullOutput, rect: Rect) -> Color32 {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    Shape::Rect(r) if r.rect == rect && r.corner_radius == CornerRadius::same(4) => Some(r.fill),
                    _ => None,
                })
                .unwrap()
        }
        let (idle, response) = frame(vec![], true);
        let center = response.rect.center();
        assert_eq!(fill(&idle, response.rect), Color32::TRANSPARENT);
        frame(vec![Event::PointerMoved(center)], true);
        let mut hovered = frame(vec![], true);
        for _ in 0..4 {
            hovered = frame(vec![], true);
        }
        let hover_fill = fill(&hovered.0, response.rect);
        assert!(hover_fill.a() > 0);
        assert_eq!(hovered.0.platform_output.cursor_icon, CursorIcon::PointingHand);
        frame(vec![Event::PointerButton { pos: center, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }], true);
        let pressed = frame(vec![], true);
        assert!(fill(&pressed.0, response.rect).a() > hover_fill.a());
        let (_, released) =
            frame(vec![Event::PointerButton { pos: center, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }], true);
        assert!(released.clicked());
        let (disabled, response) = frame(vec![Event::PointerMoved(center)], false);
        assert_eq!(fill(&disabled, response.rect), Color32::TRANSPARENT);
        assert!(!response.clicked());
    }
}

fn exercise_application_details() {
    let mut app = fixture();
    let mut data = (*app.data).clone();
    data.sampled = 1;
    data.uptime = 3600;
    data.mem_available = 512;
    for (i, p) in data.processes.iter_mut().enumerate() {
        p.memory = (i as u64 + 1) * 100;
        p.cpu = i as f32 + 1.0;
        p.io_available = true;
        p.read_rate = 1024.0;
        p.threads = 2;
    }
    app.data = Arc::new(data);
    let ctx = Context::default();
    render(&mut app, &ctx, vec![], false);
    let output = render(&mut app, &ctx, vec![], false);
    fn count_text(shape: &Shape, label: &str) -> usize {
        match shape {
            Shape::Text(text) => usize::from(text.galley.text() == label),
            Shape::Vec(shapes) => shapes.iter().map(|s| count_text(s, label)).sum(),
            _ => 0,
        }
    }
    assert_eq!(
        output.shapes.iter().map(|s| count_text(&s.shape, "50%")).sum::<usize>(),
        2,
        "Memory and Total memory headers show the same system percentage"
    );
    let parent = text_position(&output, "parent").unwrap() + vec2(15.0, 7.0);
    click(&mut app, &ctx, parent, PointerButton::Primary, false);
    assert!(app.application.is_none(), "single click only selects");
    click(&mut app, &ctx, parent, PointerButton::Primary, false);
    let view = app.application.as_ref().expect("double click opens application details");
    assert_eq!(view.key(), (10, 10));
    assert_eq!(view.rows.len(), 3);
    assert_eq!(view.cpu, 6.0);
    assert_eq!(view.memory, 600);
    assert_eq!(view.io, Some(3072.0));
    assert_eq!(view.threads, 6);
    let output = render(&mut app, &ctx, vec![], false);
    assert!(text_position(&output, "Application details").is_some());
    assert!(text_position(&output, "Subprocesses (2)").is_some());
    let properties = text_position(&output, "Properties").unwrap() + vec2(7.0, 7.0);
    click(&mut app, &ctx, properties, PointerButton::Primary, false);
    assert!(app.properties.is_some(), "Properties opens a working dialog");
    let output = render(&mut app, &ctx, vec![], false);
    assert!(text_position(&output, "Reduce priority").is_some());
    app.properties = None;
    let output = render(&mut app, &ctx, vec![], false);
    let back = text_position(&output, "<  Back").unwrap() + vec2(7.0, 7.0);
    click(&mut app, &ctx, back, PointerButton::Primary, false);
    assert!(app.application.is_none());
    assert_eq!(app.page, Page::Processes);
    app.search = "parent".into();
    app.open_application(app.data.processes[0].clone());
    let view = app.application.as_mut().unwrap();
    let mut data = (*app.data).clone();
    data.sampled += 1;
    data.processes[1].io_available = false;
    view.refresh(&data, false);
    assert!(view.io.is_none(), "partial disk counters do not masquerade as complete totals");
    assert!(view.io_history.is_empty());
    let samples = view.cpu_history.len();
    view.refresh(&data, false);
    assert_eq!(view.cpu_history.len(), samples, "repaints do not create history");
    for sample in 3..100 {
        data.sampled = sample;
        view.refresh(&data, false);
    }
    assert_eq!(view.cpu_history.len(), HISTORY);
    data.processes[0].start += 1;
    data.sampled += 1;
    view.refresh(&data, false);
    assert!(!view.running, "a reused PID is not mistaken for the original application");
    assert_eq!(view.memory, 0);
    assert_eq!(view.cpu, 0.0);
    app.data = Arc::new(data);
    let output = render(&mut app, &ctx, vec![], false);
    assert!(text_position(&output, "This process has exited. Its last details are kept until you go back.").is_some());
    app.ask_end();
    assert!(app.kill_dialog.is_none(), "stale identities cannot be terminated");
    app.close_application();
    assert_eq!(app.search, "parent", "Back restores the list search");
    app.open_application(app.data.processes[0].clone());
    app.metadata_busy = true;
    app.tx.send(Worker::Application((10, 10), app_details::Metadata { version: "stale".into(), ..Default::default() })).unwrap();
    app.update_data();
    assert!(!app.metadata_busy);
    assert!(app.application.as_ref().unwrap().metadata.is_none(), "late metadata for a reused PID is ignored");
    let child = app.data.processes[1].clone();
    app.open_application(child.clone());
    assert_eq!(app.application.as_ref().unwrap().key(), (child.pid, child.start));
    assert_eq!(app.selected, Some((child.pid, child.start)), "child actions retain the child's exact identity");
    app.settings.low_memory = true;
    app.open_application(app.data.processes[0].clone());
    assert!(app.application.as_ref().unwrap().cpu_history.is_empty());
    app.switch(Page::Apps);
    assert!(app.application.is_none(), "sidebar navigation leaves details");
}

fn exercise_application_updates_parent_and_network() {
    let mut app = fixture();
    let ctx = Context::default();
    app.open_application(app.data.processes[1].clone());
    app.application.as_mut().unwrap().metadata =
        Some(app_details::Metadata { format: "deb".into(), package: "app".into(), version: "1".into(), ..Default::default() });
    render(&mut app, &ctx, vec![], false);
    let out = render(&mut app, &ctx, vec![], false);
    assert!(text_position(&out, "Check for updates").is_some());
    assert!(text_position(&out, "Update").is_none(), "only show Update after a candidate has been found");
    let link = text_position(&out, "parent (PID 10) >").unwrap() + vec2(8.0, 7.0);
    click(&mut app, &ctx, link, PointerButton::Primary, false);
    assert_eq!(app.application.as_ref().unwrap().key(), (10, 10), "parent link navigates to exact live parent");
    let target = updates::Target { format: "deb".into(), package: "app".into(), scope: String::new() };
    app.tx.send(Worker::UpdateCheck((11, 11), Ok(Some(updates::Available { version: "wrong".into(), target: target.clone() })))).unwrap();
    app.update_data();
    assert!(matches!(app.application.as_ref().unwrap().update, application_page::UpdateState::Idle), "ignore responses for another process");
    app.tx.send(Worker::UpdateCheck((10, 10), Ok(Some(updates::Available { version: "2".into(), target })))).unwrap();
    app.update_data();
    render(&mut app, &ctx, vec![], false);
    let out = render(&mut app, &ctx, vec![], false);
    let button = text_position(&out, "Update").unwrap() + vec2(8.0, 7.0);
    click(&mut app, &ctx, button, PointerButton::Primary, false);
    assert!(
        matches!(app.authorization, Some(PrivilegedAction::Update { key: (10, 10), .. })),
        "system update requests authorization for the selected package"
    );
    app.authorization = None;
    app.tx.send(Worker::UpdateCheck((10, 10), Err("offline".into()))).unwrap();
    app.update_data();
    assert!(matches!(app.application.as_ref().unwrap().update, application_page::UpdateState::Error(_)), "failed queries must not report up to date");
    app.tx.send(Worker::Updated((11, 11), Ok(()))).unwrap();
    app.update_data();
    assert!(
        matches!(app.application.as_ref().unwrap().update, application_page::UpdateState::Error(_)),
        "late install results cannot change another view"
    );
    let mut data = (*app.data).clone();
    data.processes[0].start = 99;
    app.data = Arc::new(data);
    app.open_application(app.data.processes[1].clone());
    let out = render(&mut app, &ctx, vec![], false);
    assert!(text_position(&out, "parent (PID 10) >").is_none(), "recycled parent is not navigable");
    app.close_application();
    app.settings.columns = vec![Column::Name, Column::Network];
    app.tx.send(Worker::Network((10, 10), Ok(99.0))).unwrap();
    app.tx.send(Worker::Network((11, 11), Ok(1024.0))).unwrap();
    app.update_data();
    assert!(!app.network_samples.contains_key(&(10, 10)), "stale measurement cannot attach to a recycled PID");
    assert!(app.network_samples.contains_key(&(11, 11)));
    render(&mut app, &ctx, vec![], false);
    let out = render(&mut app, &ctx, vec![], false);
    let sample = text_position(&out, "1 KB/s *").unwrap() + vec2(5.0, 7.0);
    click(&mut app, &ctx, sample, PointerButton::Primary, false);
    assert!(matches!(app.authorization, Some(PrivilegedAction::Network { pid: 11, start: 11, .. })), "network cells offer authenticated read");
    app.authorization = None;
    app.network_samples.insert((11, 11), (1024.0, Instant::now() - Duration::from_secs(11)));
    app.update_data();
    assert!(app.network_samples.is_empty(), "old samples must not look like live counters");
}
