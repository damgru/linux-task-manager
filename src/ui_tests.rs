use super::*;
fn fixture() -> TaskManager {
    let (tx, rx) = mpsc::channel();
    let process = |pid, parent, name: &str| Process { pid, parent, start: pid as u64, name: name.into(), ..Default::default() };
    TaskManager {
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
                app.process_table(ui, false)
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
