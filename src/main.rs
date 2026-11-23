#![windows_subsystem = "windows"]

mod icon;

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const DEP_DIRS: &[&str] = &[
    "node_modules", "bower_components", ".venv", "venv", "__pycache__", ".tox", ".mypy_cache",
    ".pytest_cache", ".ruff_cache", ".gradle", "Pods", "vendor", ".next", ".nuxt",
    ".parcel-cache", ".dart_tool", ".svelte-kit", ".angular", ".turbo", "__pypackages__",
];

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    path: String,
    parent: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    entries: Vec<Entry>,
    interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config { entries: vec![], interval_secs: 60 }
    }
}

fn config_path() -> PathBuf {
    let d = dirs::config_dir().unwrap_or_default().join("shtstain");
    let _ = std::fs::create_dir_all(&d);
    d.join("config.json")
}

const FILTERS: &[&str] = &["All projects", "Has dependencies", "No dependencies", "Deps over 100 MB", "Deps over 1 GB"];

fn filter_ok(f: usize, p: &Project) -> bool {
    let d = p.dep_total();
    match f {
        1 => d > 0,
        2 => d == 0,
        3 => d > 100 << 20,
        4 => d > 1 << 30,
        _ => true,
    }
}

fn load_config() -> Config {
    let mut c: Config = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    // Legacy parent entries become individual project entries.
    let mut out: Vec<Entry> = vec![];
    for e in c.entries {
        if e.parent {
            for d in std::fs::read_dir(&e.path).into_iter().flatten().flatten() {
                let n = d.file_name().to_string_lossy().into_owned();
                if d.file_type().is_ok_and(|t| t.is_dir()) && !n.starts_with('.') {
                    out.push(Entry { path: d.path().to_string_lossy().into_owned(), parent: false });
                }
            }
        } else {
            out.push(e);
        }
    }
    out.dedup_by(|a, b| a.path == b.path);
    c.entries = out;
    c
}

fn save_config(c: &Config) {
    if let Ok(s) = serde_json::to_string_pretty(c) {
        let _ = std::fs::write(config_path(), s);
    }
}

#[derive(Clone, Default)]
struct Project {
    name: String,
    path: String,
    source: u64,
    deps: Vec<(String, u64)>,
}

impl Project {
    fn dep_total(&self) -> u64 {
        self.deps.iter().map(|d| d.1).sum()
    }
    fn total(&self) -> u64 {
        self.source + self.dep_total()
    }
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            if let Ok(ft) = e.file_type() {
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    total += dir_size(&e.path());
                } else if let Ok(m) = e.metadata() {
                    total += m.len();
                }
            }
        }
    }
    total
}

fn is_dep(name: &str, path: &Path) -> bool {
    DEP_DIRS.contains(&name)
        || path.join("pyvenv.cfg").exists()
        || (name == "target" && path.parent().is_some_and(|p| p.join("Cargo.toml").exists()))
}

fn walk(dir: &Path, src: &mut u64, deps: &mut Vec<(String, u64)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let p = e.path();
        if ft.is_dir() {
            if is_dep(&e.file_name().to_string_lossy(), &p) {
                deps.push((p.to_string_lossy().into_owned(), dir_size(&p)));
            } else {
                walk(&p, src, deps);
            }
        } else if let Ok(m) = e.metadata() {
            *src += m.len();
        }
    }
}

fn scan_project(p: PathBuf) -> Project {
    let mut pr = Project {
        name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned()),
        path: p.to_string_lossy().into_owned(),
        ..Default::default()
    };
    walk(&p, &mut pr.source, &mut pr.deps);
    pr.deps.sort_by(|a, b| b.1.cmp(&a.1));
    pr
}

fn scan_all(cfg: &Config) -> Vec<Project> {
    let mut roots: Vec<PathBuf> = vec![];
    for e in &cfg.entries {
        let p = PathBuf::from(&e.path);
        if e.parent {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for c in rd.flatten() {
                    let n = c.file_name().to_string_lossy().into_owned();
                    if c.file_type().is_ok_and(|t| t.is_dir()) && !n.starts_with('.') {
                        roots.push(c.path());
                    }
                }
            }
        } else {
            roots.push(p);
        }
    }
    roots.sort();
    roots.dedup();
    let mut out: Vec<Project> = std::thread::scope(|s| {
        let hs: Vec<_> = roots.into_iter().map(|r| s.spawn(move || scan_project(r))).collect();
        hs.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    out.sort_by(|a, b| b.total().cmp(&a.total()));
    out
}

fn autostart_enabled() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg")
        .args(["query", RUN_KEY, "/v", "shtstain"])
        .creation_flags(0x0800_0000)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn set_autostart(on: bool) {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new("reg");
    if on {
        let exe = std::env::current_exe().unwrap_or_default();
        let val = format!("\"{}\" --tray", exe.display());
        c.args(["add", RUN_KEY, "/v", "shtstain", "/t", "REG_SZ", "/d", &val, "/f"]);
    } else {
        c.args(["delete", RUN_KEY, "/v", "shtstain", "/f"]);
    }
    let _ = c.creation_flags(0x0800_0000).output();
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

fn fmt(b: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", units[i]) }
}

static TOTAL: AtomicU64 = AtomicU64::new(0);
static DEPS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static SIZE_ITEM: RefCell<Option<MenuItem>> = const { RefCell::new(None) };
}

fn refresh_tray_text() {
    SIZE_ITEM.with(|i| {
        if let Some(i) = i.borrow().as_ref() {
            i.set_text(format!(
                "Occupied: {}  (deps: {})",
                fmt(TOTAL.load(Ordering::Relaxed)),
                fmt(DEPS.load(Ordering::Relaxed))
            ));
        }
    });
}

fn make_icon() -> Icon {
    Icon::from_rgba(icon::rgba(48, icon::system_light("SystemUsesLightTheme")), 48, 48).unwrap()
}

struct Shared {
    projects: Vec<Project>,
    scanning: bool,
    last_scan: Option<std::time::Instant>,
}

struct App {
    cfg: Config,
    shared: Arc<Mutex<Shared>>,
    tx: Sender<()>,
    autostart: bool,
    search: String,
    filter: usize,
    sort_col: usize,
    sort_asc: bool,
    expanded: std::collections::HashSet<String>,
    pending: Option<(String, Vec<String>)>,
    cfg_tx: Sender<Config>,
    _tray: TrayIcon,
}

fn scanner(rx: Receiver<()>, cfg_rx: Receiver<Config>, mut cfg: Config, shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    loop {
        while let Ok(c) = cfg_rx.try_recv() {
            cfg = c;
        }
        shared.lock().unwrap().scanning = true;
        ctx.request_repaint();
        let projects = scan_all(&cfg);
        TOTAL.store(projects.iter().map(|p| p.total()).sum(), Ordering::Relaxed);
        DEPS.store(projects.iter().map(|p| p.dep_total()).sum(), Ordering::Relaxed);
        {
            let mut s = shared.lock().unwrap();
            s.projects = projects;
            s.scanning = false;
            s.last_scan = Some(std::time::Instant::now());
        }
        ctx.request_repaint();
        let _ = rx.recv_timeout(Duration::from_secs(cfg.interval_secs.max(5)));
    }
}

fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
        let cfg = load_config();
        let shared = Arc::new(Mutex::new(Shared { projects: vec![], scanning: true, last_scan: None }));
        let (tx, rx) = mpsc::channel();
        let (cfg_tx, cfg_rx) = mpsc::channel();
        {
            let (c, s, ctx) = (cfg.clone(), shared.clone(), cc.egui_ctx.clone());
            std::thread::spawn(move || scanner(rx, cfg_rx, c, s, ctx));
        }

        let size_item = MenuItem::new("Occupied: scanning...", false, None);
        let open_item = MenuItem::new("Open", true, None);
        let exit_item = MenuItem::new("Exit", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&size_item, &PredefinedMenuItem::separator(), &open_item, &exit_item]);
        let (open_id, exit_id) = (open_item.id().clone(), exit_item.id().clone());
        SIZE_ITEM.with(|i| *i.borrow_mut() = Some(size_item));

        let ctx = cc.egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == exit_id {
                std::process::exit(0);
            } else if e.id == open_id {
                show_window(&ctx);
            }
        }));
        let ctx = cc.egui_ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            refresh_tray_text();
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_window(&ctx);
            }
        }));
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("sh*tstain")
            .with_icon(make_icon())
            .build()
            .expect("tray");

        App { cfg, shared, tx, autostart: autostart_enabled(), search: String::new(), filter: 0, sort_col: 3, sort_asc: false, expanded: Default::default(), pending: None, cfg_tx, _tray: tray }
    }

    fn changed(&self) {
        save_config(&self.cfg);
        let _ = self.cfg_tx.send(self.cfg.clone());
        let _ = self.tx.send(());
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let fill = root.visuals().panel_fill;
        egui::Frame::new().fill(fill).inner_margin(14.0).show(root, |ui| self.body(ui));
    }
}

impl App {
    fn body(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        refresh_tray_text();
        ctx.request_repaint_after(Duration::from_secs(2));
        let (projects, scanning, last) = {
            let s = self.shared.lock().unwrap();
            (s.projects.clone(), s.scanning, s.last_scan)
        };
        let total = TOTAL.load(Ordering::Relaxed);
        let deps = DEPS.load(Ordering::Relaxed);

        let mut add: Vec<PathBuf> = vec![];
        ui.horizontal(|ui| {
            if scanning {
                ui.spinner();
                ui.weak("scanning...");
            } else if let Some(l) = last {
                ui.weak(format!("scanned {}s ago", l.elapsed().as_secs()));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let arrow = ui.button("v");
                egui::Popup::menu(&arrow).show(|ui| {
                    if ui.button("Add Multiple from folder").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            add.extend(
                                std::fs::read_dir(&p)
                                    .into_iter()
                                    .flatten()
                                    .flatten()
                                    .filter(|c| c.file_type().is_ok_and(|t| t.is_dir()) && !c.file_name().to_string_lossy().starts_with('.'))
                                    .map(|c| c.path()),
                            );
                        }
                        ui.close();
                    }
                });
                if ui.button("Add Project").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        add.push(p);
                    }
                }
                ui.menu_button("Settings", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Rescan every");
                        let mut secs = self.cfg.interval_secs;
                        if ui.add(egui::DragValue::new(&mut secs).range(5..=3600).suffix(" s")).changed() {
                            self.cfg.interval_secs = secs;
                            save_config(&self.cfg);
                            let _ = self.cfg_tx.send(self.cfg.clone());
                        }
                    });
                    if ui.button("Scan now").clicked() {
                        let _ = self.tx.send(());
                        ui.close();
                    }
                    if ui.checkbox(&mut self.autostart, "Start with Windows").changed() {
                        set_autostart(self.autostart);
                    }
                });
            });
        });
        if !add.is_empty() {
            for p in add {
                let path = p.to_string_lossy().into_owned();
                if !self.cfg.entries.iter().any(|e| e.path == path) {
                    self.cfg.entries.push(Entry { path, parent: false });
                }
            }
            self.changed();
        }

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search projects...").desired_width(220.0));
            egui::ComboBox::from_id_salt("filter").selected_text(FILTERS[self.filter]).show_ui(ui, |ui| {
                for (i, f) in FILTERS.iter().enumerate() {
                    ui.selectable_value(&mut self.filter, i, *f);
                }
            });
        });

        let q = self.search.to_lowercase();
        let mut projects: Vec<Project> = projects
            .into_iter()
            .filter(|p| (q.is_empty() || p.name.to_lowercase().contains(&q) || p.path.to_lowercase().contains(&q)) && filter_ok(self.filter, p))
            .collect();
        projects.sort_by(|a, b| {
            let o = match self.sort_col {
                0 => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                1 => a.source.cmp(&b.source),
                2 => a.dep_total().cmp(&b.dep_total()),
                _ => a.total().cmp(&b.total()),
            };
            if self.sort_asc { o } else { o.reverse() }
        });

        ui.separator();
        ui.strong(format!(
            "Total {}   |   source {}   |   dependencies {}",
            fmt(total),
            fmt(total.saturating_sub(deps)),
            fmt(deps)
        ));
        ui.separator();
        let mut remove: Option<String> = None;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("projects").striped(true).num_columns(5).spacing([16.0, 6.0]).show(ui, |ui| {
                for (i, h) in ["Project", "Source", "Dependencies", "Total"].iter().enumerate() {
                    let mark = if self.sort_col == i { if self.sort_asc { " ^" } else { " v" } } else { "" };
                    let r = ui.add(egui::Label::new(egui::RichText::new(format!("{h}{mark}")).strong()).sense(egui::Sense::click()));
                    if r.on_hover_text("Click to sort").clicked() {
                        if self.sort_col == i {
                            self.sort_asc = !self.sort_asc;
                        } else {
                            self.sort_col = i;
                            self.sort_asc = i == 0;
                        }
                    }
                }
                ui.label("");
                ui.end_row();
                for p in &projects {
                    let open = self.expanded.contains(&p.path);
                    let arrow = if p.deps.is_empty() { "  " } else if open { "- " } else { "+ " };
                    let r = ui.add(egui::Label::new(format!("{arrow}{}", p.name)).sense(egui::Sense::click()));
                    if r.on_hover_text(&p.path).clicked() && !p.deps.is_empty() {
                        if open {
                            self.expanded.remove(&p.path);
                        } else {
                            self.expanded.insert(p.path.clone());
                        }
                    }
                    ui.label(fmt(p.source));
                    ui.label(fmt(p.dep_total()));
                    ui.label(fmt(p.total()));
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!p.deps.is_empty(), |ui| {
                            if ui.button("Delete deps").clicked() {
                                self.pending = Some((p.name.clone(), p.deps.iter().map(|d| d.0.clone()).collect()));
                            }
                        });
                        if self.cfg.entries.iter().any(|e| e.path == p.path)
                            && ui.button("Remove").on_hover_text("Stop watching this folder (files are kept)").clicked()
                        {
                            remove = Some(p.path.clone());
                        }
                    });
                    ui.end_row();
                    if open {
                        for (d, sz) in &p.deps {
                            let rel = d.strip_prefix(&p.path).unwrap_or(d).trim_start_matches('\\');
                            ui.weak(format!("      {rel}"));
                            ui.label("");
                            ui.weak(fmt(*sz));
                            ui.label("");
                            if ui.small_button("Delete").clicked() {
                                self.pending = Some((rel.to_string(), vec![d.clone()]));
                            }
                            ui.end_row();
                        }
                    }
                }
            });
            if projects.is_empty() && !scanning {
                ui.label("No folders yet. Add one above.");
            }
        });

        if let Some(path) = remove {
            self.cfg.entries.retain(|e| e.path != path);
            self.changed();
        }

        if let Some((name, paths)) = self.pending.clone() {
            let mut close = false;
            egui::Window::new("Delete dependencies?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                ui.label(format!("Permanently delete {} folder(s) from \"{name}\"?", paths.len()));
                for p in &paths {
                    ui.weak(p);
                }
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        let tx = self.tx.clone();
                        let paths = paths.clone();
                        std::thread::spawn(move || {
                            for p in paths {
                                let _ = std::fs::remove_dir_all(p);
                            }
                            let _ = tx.send(());
                        });
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if close {
                self.pending = None;
            }
        }
    }
}

fn main() -> eframe::Result {
    let hidden = std::env::args().any(|a| a == "--tray");
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 520.0]).with_title("sh*tstain")
            .with_icon(egui::IconData { rgba: icon::rgba(64, icon::system_light("AppsUseLightTheme")), width: 64, height: 64 }).with_visible(!hidden),
        ..Default::default()
    };
    eframe::run_native("sh*tstain", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
