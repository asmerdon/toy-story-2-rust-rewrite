//! OpenTS2 launcher: the window players start from. It finds their Toy Story 2 install,
//! then starts the engine (`ts2-viewer`) for the original game, the modded game or the level
//! tester. The logic lives in the library (`opents2_launcher`); this file is the window.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use eframe::egui;
use opents2_launcher::config::Config;
use opents2_launcher::install::{self, DiscMedia, GameDirError};
use opents2_launcher::launch::{self, Mode};
use opents2_launcher::{iso, mods, paths, DISCLAIMER, VERSION};

/// The play buttons' colour.
const PLAY_GREEN: egui::Color32 = egui::Color32::from_rgb(46, 140, 64);

const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon/opents2.png");

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(ICON_PNG).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("OpenTS2")
        .with_inner_size([760.0, 520.0])
        .with_min_inner_size([560.0, 420.0]);
    if let Some(icon) = icon.clone() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("OpenTS2", options, Box::new(move |cc| Ok(Box::new(Launcher::new(cc, icon)))))
}

/// Music and movies being copied out of a disc image in the background.
struct DiscCopy {
    done: Arc<AtomicU64>,
    total: u64,
    task: JoinHandle<std::io::Result<()>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Play,
    GameFiles,
    Mods,
    LevelTester,
    About,
}

struct Launcher {
    config: Config,
    tab: Tab,
    game: Result<(), GameDirError>,
    disc: Option<DiscMedia>,
    /// A disc image in the chosen disc folder (when it has no loose media) and its media size.
    image: Option<(PathBuf, u64)>,
    copying: Option<DiscCopy>,
    found_mods: Vec<mods::Mod>,
    running: Option<(Child, &'static str)>,
    status: String,
    logo: Option<egui::TextureHandle>,
}

impl Launcher {
    fn new(cc: &eframe::CreationContext<'_>, icon: Option<egui::IconData>) -> Self {
        let logo = icon.map(|i| {
            let img = egui::ColorImage::from_rgba_unmultiplied([i.width as usize, i.height as usize], &i.rgba);
            cc.egui_ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
        });
        let mut config = Config::load(&paths::config_file());
        let mut status = String::new();
        if config.game_dir.as_deref().is_none_or(|g| install::check_game_dir(g).is_err())
            && let Some(found) = install::detect_game_dir()
        {
            status = format!("Found Toy Story 2 at {}", found.display());
            config.game_dir = Some(found);
        }
        // Music and movies copied from a disc image earlier.
        if config.cd_dir.is_none() && install::find_disc_media(&paths::disc_copy_dir()).is_some() {
            config.cd_dir = Some(paths::disc_copy_dir());
        }
        let mut me = Self {
            config,
            tab: Tab::Play,
            game: Err(GameDirError::NotAFolder),
            disc: None,
            image: None,
            copying: None,
            found_mods: Vec::new(),
            running: None,
            status,
            logo,
        };
        me.refresh();
        if me.game.is_err() {
            me.tab = Tab::GameFiles;
        }
        me
    }

    /// Re-checks the folders and the mods, and saves the settings.
    fn refresh(&mut self) {
        self.game = match &self.config.game_dir {
            Some(g) => install::check_game_dir(g),
            None => Err(GameDirError::NotAFolder),
        };
        self.disc = self.config.cd_dir.as_deref().and_then(install::find_disc_media);
        self.image = match (&self.disc, &self.config.cd_dir) {
            (None, Some(dir)) => iso::images_in(dir).into_iter().find_map(|p| iso::media_size(&p).ok().filter(|&n| n > 0).map(|n| (p, n))),
            _ => None,
        };
        if let Some(bundled) = paths::bundled_mods_dir() {
            let copied = mods::install_examples(&bundled, &paths::mods_dir(), &self.config.examples);
            self.config.examples.extend(copied);
        }
        self.found_mods = mods::scan(&paths::mods_dir());
        self.config.mods = mods::reconcile(&self.config.mods, &self.found_mods);
        self.save();
    }

    fn save(&mut self) {
        if let Err(e) = self.config.save(&paths::config_file()) {
            self.status = format!("Couldn't save settings: {e}");
        }
    }

    fn start(&mut self, mode: Mode, label: &'static str) {
        let Some(game) = self.config.game_dir.clone().filter(|_| self.game.is_ok()) else {
            self.status = "Choose your Toy Story 2 folder first (Game Files).".into();
            self.tab = Tab::GameFiles;
            return;
        };
        let engine = paths::engine_exe();
        if !engine.is_file() {
            self.status = format!("The engine is missing: {}. Reinstall OpenTS2.", engine.display());
            return;
        }
        let cd = self.disc.as_ref().map(|d| d.root.clone());
        let plan = launch::plan(&engine, &game, cd.as_deref(), &paths::home(), &mode);
        match plan.spawn(&paths::log_file()) {
            Ok(child) => {
                self.status = format!("{label} running.");
                self.running = Some((child, label));
            }
            Err(e) => self.status = format!("Couldn't start the engine: {e}"),
        }
    }

    fn poll_running(&mut self, ctx: &egui::Context) {
        let Some((child, label)) = &mut self.running else { return };
        match child.try_wait() {
            Ok(None) => ctx.request_repaint_after(Duration::from_millis(500)),
            Ok(Some(code)) if code.success() => {
                self.status = format!("{label} closed.");
                self.running = None;
            }
            Ok(Some(code)) => {
                self.status = format!("{label} stopped with an error ({code}). Details: {}", paths::log_file().display());
                self.running = None;
            }
            Err(e) => {
                self.status = format!("Lost track of the game: {e}");
                self.running = None;
            }
        }
    }

    fn start_copy(&mut self) {
        let Some((image, total)) = self.image.clone() else { return };
        let done = Arc::new(AtomicU64::new(0));
        let d = done.clone();
        let task = std::thread::spawn(move || iso::copy_media(&image, &paths::disc_copy_dir(), &d));
        self.copying = Some(DiscCopy { done, total, task });
        self.status = "Copying music and movies from the disc image…".into();
    }

    fn poll_copy(&mut self, ctx: &egui::Context) {
        if !self.copying.as_ref().is_some_and(|c| c.task.is_finished()) {
            if self.copying.is_some() {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            return;
        }
        let copy = self.copying.take().unwrap();
        match copy.task.join() {
            Ok(Ok(())) => {
                self.config.cd_dir = Some(paths::disc_copy_dir());
                self.status = "Music and movies copied from the disc image.".into();
                self.refresh();
            }
            Ok(Err(e)) => self.status = format!("Couldn't copy from the disc image: {e}"),
            Err(_) => self.status = "Copying from the disc image failed.".into(),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(60.0, 60.0)));
            }
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.heading(egui::RichText::new("OpenTS2").size(28.0).strong());
                ui.label(format!("An open engine for Toy Story 2 · version {VERSION}"));
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (tab, name) in [
                (Tab::Play, "Play"),
                (Tab::GameFiles, "Game Files"),
                (Tab::Mods, "Mods"),
                (Tab::LevelTester, "Level Tester"),
                (Tab::About, "About"),
            ] {
                ui.selectable_value(&mut self.tab, tab, name);
            }
        });
    }

    fn play_tab(&mut self, ui: &mut egui::Ui) {
        // The token, large, to the right of the buttons when the window is wide enough.
        let area = ui.max_rect();
        let side = (area.height() - 60.0).min(area.width() - 320.0).min(260.0);
        if let Some(logo) = &self.logo
            && side >= 120.0
        {
            let rect = egui::Rect::from_min_size(egui::pos2(area.right() - side - 16.0, area.top() + 50.0), egui::vec2(side, side));
            egui::Image::new(logo).paint_at(ui, rect);
        }
        let ready = self.game.is_ok() && self.running.is_none();
        match (&self.game, &self.config.game_dir) {
            (Ok(()), Some(g)) => ui.label(format!("Game: {}", g.display())),
            _ => ui.colored_label(ui.visuals().warn_fg_color, "Toy Story 2 isn't set up yet: open Game Files."),
        };
        ui.label(match &self.disc {
            Some(d) if d.music && d.movies => "Music and movies: from the disc folder.",
            Some(d) if d.music => "Music: from the disc folder (no movies found).",
            Some(_) => "Movies: from the disc folder (no music found).",
            None if self.image.is_some() => "Disc image found: copy its music and movies on Game Files.",
            None => "No disc folder: the game runs without music and movies.",
        });
        ui.add_space(12.0);
        if ui.add_enabled(ready, play_button("▶  Play Original")).on_hover_text("The faithful port, from the startup screens").clicked() {
            self.start(Mode::Original, "Toy Story 2");
        }
        ui.add_space(6.0);
        let n = self.config.enabled_mods().count();
        if ui.add_enabled(ready, big_button("Play Modded")).on_hover_text(format!("The game with {n} enabled mod(s); see Mods")).clicked() {
            self.start_modded();
        }
        ui.add_space(6.0);
        if ui.add_enabled(ready, big_button("Level Tester")).on_hover_text("Fly around any level; F plays as Buzz").clicked() {
            self.tab = Tab::LevelTester;
        }
        if self.running.is_some() {
            ui.add_space(12.0);
            if ui.button("Stop the game").clicked()
                && let Some((child, _)) = &mut self.running
            {
                let _ = child.kill();
            }
        }
    }

    /// The enabled mods' folders, in load order.
    fn enabled_mod_dirs(&self) -> Vec<PathBuf> {
        self.config
            .enabled_mods()
            .filter_map(|id| self.found_mods.iter().find(|m| m.id() == id).map(|m| m.dir.clone()))
            .collect()
    }

    fn start_modded(&mut self) {
        let dirs = self.enabled_mod_dirs();
        self.start(Mode::Modded(dirs), "Modded Toy Story 2");
    }

    /// The level tester, with the enabled mods if any.
    fn start_level(&mut self, n: usize) {
        let dirs = self.enabled_mod_dirs();
        self.start(Mode::LevelTester(n, dirs), "Level Tester");
    }

    fn game_files_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Toy Story 2 folder");
        ui.label("OpenTS2 plays from your own installed copy of the PC game. Nothing is copied or changed.");
        let mut changed = false;
        ui.horizontal(|ui| {
            let mut text = self.config.game_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut text).desired_width(420.0).hint_text(install::DEFAULT_GAME_DIR)).changed() {
                self.config.game_dir = (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
                changed = true;
            }
            if ui.button("Browse…").clicked()
                && let Some(p) = pick_folder("Choose the Toy Story 2 folder (the one with toy2.exe)", self.config.game_dir.as_deref())
            {
                self.config.game_dir = Some(p);
                changed = true;
            }
            if ui.button("Find it").on_hover_text("Look in the usual install places").clicked() {
                match install::detect_game_dir() {
                    Some(p) => {
                        self.config.game_dir = Some(p);
                        changed = true;
                    }
                    None => self.status = "No install found in the usual places; use Browse.".into(),
                }
            }
        });
        match &self.game {
            Ok(()) => ui.colored_label(egui::Color32::from_rgb(60, 170, 80), "✔ Toy Story 2 found."),
            Err(e) => ui.colored_label(ui.visuals().error_fg_color, format!("✖ {e}")),
        };

        ui.add_space(16.0);
        ui.heading("Disc folder (optional)");
        ui.label("For music and movies: the game disc, a copy of it, or a disc image (.iso).");
        let idle = self.copying.is_none();
        ui.add_enabled_ui(idle, |ui| {
            ui.horizontal(|ui| {
                let mut text = self.config.cd_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut text).desired_width(420.0).hint_text(r"D:\")).changed() {
                    self.config.cd_dir = (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
                    changed = true;
                }
                if ui.button("Browse…").clicked()
                    && let Some(p) = pick_folder("Choose the game disc, its copy, or the folder with its .iso", self.config.cd_dir.as_deref())
                {
                    self.config.cd_dir = Some(install::find_disc_media(&p).map(|d| d.root).unwrap_or(p));
                    changed = true;
                }
                if ui.button("Disc image…").on_hover_text("Choose a .iso file").clicked()
                    && let Some(p) = pick_iso(self.config.cd_dir.as_deref())
                {
                    self.config.cd_dir = Some(p);
                    changed = true;
                }
                if self.config.cd_dir.is_some() && ui.button("Clear").clicked() {
                    self.config.cd_dir = None;
                    changed = true;
                }
            });
        });
        if let Some(c) = &self.copying {
            let done = c.done.load(Ordering::Relaxed);
            let frac = done as f32 / c.total.max(1) as f32;
            ui.add(egui::ProgressBar::new(frac).show_percentage().text(format!("Copying {} of {} MB", done >> 20, c.total >> 20)));
        }
        match (&self.config.cd_dir, &self.disc, &self.image) {
            (None, _, _) => {}
            (Some(_), None, Some((image, size))) if idle => {
                let name = image.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ui.label(format!("Found the disc image {name}. OpenTS2 needs its music and movies as files: copy them once ({} MB) into {}.", size >> 20, paths::disc_copy_dir().display()));
                if ui.add(egui::Button::new(egui::RichText::new("Copy Music and Movies").strong())).clicked() {
                    self.start_copy();
                }
            }
            (Some(_), None, Some(_)) => {}
            (Some(_), Some(d), _) => {
                let what = match (d.music, d.movies) {
                    (true, true) => "✔ Music and movies found.",
                    (true, false) => "✔ Music found; no movies (rtlibs\\).",
                    _ => "✔ Movies found; no music (audio\\).",
                };
                ui.colored_label(egui::Color32::from_rgb(60, 170, 80), what);
            }
            (Some(_), None, None) => {
                ui.colored_label(ui.visuals().error_fg_color, "✖ No audio\\ or rtlibs\\ folder or Toy Story 2 disc image here.");
            }
        }
        if changed {
            self.refresh();
        }
    }

    fn mods_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mods");
        ui.label("Each mod is a folder with a mod.toml, kept outside the game. Tick the ones you want, then Play Modded. Later mods override earlier ones.");
        ui.label("The engine loads play-as mods (one character at a time). Replacing the game's files is coming later.");
        ui.horizontal(|ui| {
            ui.label(format!("Folder: {}", paths::mods_dir().display()));
            if ui.button("Open").clicked() {
                let _ = std::fs::create_dir_all(paths::mods_dir());
                open_folder(&paths::mods_dir());
            }
            if ui.button("Rescan").clicked() {
                self.refresh();
            }
        });
        ui.separator();
        if self.config.mods.is_empty() {
            ui.label("No mods installed.");
        }
        let mut changed = false;
        let mut swap = None;
        let count = self.config.mods.len();
        for i in 0..count {
            let entry = &mut self.config.mods[i];
            let found = self.found_mods.iter().find(|m| m.id() == entry.id);
            ui.horizontal(|ui| {
                changed |= ui.checkbox(&mut entry.enabled, "").changed();
                if ui.add_enabled(i > 0, egui::Button::new("▲")).clicked() {
                    swap = Some(i - 1);
                }
                if ui.add_enabled(i + 1 < count, egui::Button::new("▼")).clicked() {
                    swap = Some(i);
                }
                match found.map(|m| &m.manifest) {
                    Some(Ok(m)) => {
                        ui.label(egui::RichText::new(&m.name).strong());
                        ui.label(format!("{} {}", m.id, m.version));
                        if let Some(p) = &m.play_as {
                            ui.label(format!("plays as {}", p.creature));
                        }
                        if !m.description.is_empty() {
                            ui.label(&m.description);
                        }
                    }
                    Some(Err(e)) => {
                        ui.colored_label(ui.visuals().error_fg_color, format!("{}: bad mod.toml ({e})", entry.id));
                    }
                    None => {
                        ui.label(&entry.id);
                    }
                }
            });
        }
        if let Some(i) = swap {
            self.config.mods.swap(i, i + 1);
            changed = true;
        }
        for p in mods::order_problems(&self.config.mods, &self.found_mods) {
            ui.colored_label(ui.visuals().warn_fg_color, p);
        }
        let play_as = mods::play_as_mods(&self.config.mods, &self.found_mods);
        if let [.., (id, creature)] = play_as[..] {
            if play_as.len() > 1 {
                ui.colored_label(ui.visuals().warn_fg_color, format!("{} enabled mods change who you play as; the last one, {id}, wins ({creature}).", play_as.len()));
            }
        }
        if changed {
            self.save();
        }
        ui.add_space(12.0);
        if ui.add_enabled(self.game.is_ok() && self.running.is_none(), big_button("Play Modded")).clicked() {
            self.start_modded();
        }
    }

    fn level_tester_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Level Tester");
        ui.label("Opens one level in a free camera. WASD flies, F plays as Buzz and back, Escape closes.");
        let enabled: Vec<String> = self.config.enabled_mods().map(str::to_owned).collect();
        let modded = !enabled.is_empty();
        if modded {
            ui.label(format!("With mods: {} (change them on the Mods tab). F plays as the mod's character.", enabled.join(", ")));
        }
        ui.add_space(8.0);
        let name = ts2::campaign::level(self.config.level).map_or("", |l| l.name);
        let ready = self.game.is_ok() && self.running.is_none();
        let label = format!("▶  Play {}. {name}{}", self.config.level, if modded { " (modded)" } else { "" });
        if ui.add_enabled(ready, play_button(&label)).on_hover_text("Or double-click a level").clicked() {
            self.start_level(self.config.level);
        }
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for l in &ts2::campaign::LEVELS {
                let picked = self.config.level == l.number;
                let r = ui.selectable_label(picked, format!("{:>2}. {}", l.number, l.name));
                if r.clicked() {
                    self.config.level = l.number;
                    self.save();
                }
                if r.double_clicked() && ready {
                    self.start_level(l.number);
                }
            }
        });
    }

    fn about_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!("OpenTS2 {VERSION}"));
        ui.label("A clean-room reimplementation of Toy Story 2: Buzz Lightyear to the Rescue (PC, 1999), written in Rust.");
        ui.hyperlink_to("Source code", "https://github.com/asmerdon/toy-story-2-rust-rewrite");
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Disclaimer").strong());
        ui.label(DISCLAIMER);
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Credits").strong());
        ui.label("OpenTS2 by lewiloon2k and contributors. Inspired by OpenMW.");
        ui.label("Built with wgpu, winit, egui, rodio, Symphonia and oxideav.");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(format!("Settings and mods: {}", paths::home().display()));
            if ui.button("Open").clicked() {
                let _ = std::fs::create_dir_all(paths::home());
                open_folder(&paths::home());
            }
        });
    }
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_running(ctx);
        self.poll_copy(ctx);
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(6.0);
            self.header(ui);
            ui.add_space(2.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.label(if self.status.is_empty() { "Ready." } else { &self.status });
            ui.add_space(2.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Play => self.play_tab(ui),
            Tab::GameFiles => self.game_files_tab(ui),
            Tab::Mods => self.mods_tab(ui),
            Tab::LevelTester => self.level_tester_tab(ui),
            Tab::About => self.about_tab(ui),
        });
    }
}

fn big_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).size(20.0)).min_size(egui::vec2(280.0, 48.0))
}

/// The main action on a tab: big, green, white text.
fn play_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).size(22.0).strong().color(egui::Color32::WHITE))
        .fill(PLAY_GREEN)
        .min_size(egui::vec2(280.0, 52.0))
}

fn pick_folder(title: &str, start: Option<&Path>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(title);
    if let Some(s) = start.filter(|s| s.is_dir()) {
        d = d.set_directory(s);
    }
    d.pick_folder()
}

fn pick_iso(start: Option<&Path>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title("Choose the Toy Story 2 disc image").add_filter("Disc image", &["iso"]);
    if let Some(s) = start.filter(|s| s.is_dir()) {
        d = d.set_directory(s);
    }
    d.pick_file()
}

fn open_folder(dir: &Path) {
    let opener = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}
