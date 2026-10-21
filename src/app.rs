//! Top-level application state: start screen ↔ loading ↔ reader.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use crate::model::Document;
use crate::settings::{Settings, Theme, book_key};
use crate::ui::home::{self, HomeAction};
use crate::ui::reader::{Reader, ReaderAction};
use crate::{fonts, formats, ui};

const SAVE_INTERVAL: Duration = Duration::from_secs(2);

enum Screen {
    Home,
    Loading {
        path: PathBuf,
        rx: Receiver<anyhow::Result<Document>>,
    },
    Reading(Box<Reader>),
}

pub struct App {
    settings: Settings,
    screen: Screen,
    error: Option<String>,
    applied_theme: Option<Theme>,
    dirty: bool,
    last_save: Instant,
    /// Dev aid (`AIREADER_SCREENSHOT=out.png`): save a window screenshot once loading has
    /// settled, then quit. The instant is the capture deadline.
    screenshot: Option<(PathBuf, Option<Instant>)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        fonts::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        // Ctrl/⌘ +/- change the book's font size instead of the UI scale.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut app = Self {
            settings: Settings::load(),
            screen: Screen::Home,
            error: None,
            applied_theme: None,
            dirty: false,
            last_save: Instant::now(),
            screenshot: std::env::var_os("AIREADER_SCREENSHOT").map(|p| (PathBuf::from(p), None)),
        };
        if let Some(path) = initial {
            app.open(path, &cc.egui_ctx);
        }
        app
    }

    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.close_book(ctx);
        self.error = None;
        let (tx, rx) = channel();
        let worker_path = path.clone();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(formats::open(&worker_path));
            ctx2.request_repaint();
        });
        self.screen = Screen::Loading { path, rx };
    }

    fn close_book(&mut self, ctx: &egui::Context) {
        if let Screen::Reading(r) = &self.screen {
            self.settings
                .positions
                .insert(book_key(&r.doc.path), r.location());
            self.dirty = true;
        }
        self.screen = Screen::Home;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("AiReader".into()));
    }

    fn poll_loading(&mut self, ctx: &egui::Context) {
        let Screen::Loading { rx, path } = &self.screen else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(doc)) => {
                self.settings.touch_recent(&doc.path, &doc.title);
                let saved = self.settings.positions.get(&book_key(&doc.path)).copied();
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "{} — AiReader",
                    doc.title
                )));
                self.screen = Screen::Reading(Box::new(Reader::new(doc, saved, ctx)));
                self.dirty = true;
            }
            Ok(Err(e)) => {
                self.error = Some(format!("{e:#}"));
                self.screen = Screen::Home;
            }
            Err(TryRecvError::Disconnected) => {
                self.error = Some(format!("加载 {} 时出错", path.display()));
                self.screen = Screen::Home;
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some((path, deadline)) = &mut self.screenshot else {
            return;
        };
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                let _ = buf.save(&*path);
            }
            self.screenshot = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        // Give background page rendering and image decoding time to finish.
        if !matches!(self.screen, Screen::Loading { .. }) {
            let due = *deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(2));
            if Instant::now() >= due {
                *deadline = Some(Instant::now() + Duration::from_secs(3600));
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.applied_theme != Some(self.settings.theme) {
            ui::apply_theme(&ctx, self.settings.theme);
            self.applied_theme = Some(self.settings.theme);
        }
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            self.open(path, &ctx);
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O))
            && let Some(path) = home::pick_file()
        {
            self.open(path, &ctx);
        }
        self.poll_loading(&ctx);
        self.screenshot_step(&ctx);

        let before = self.settings.prefs_fingerprint();

        match &mut self.screen {
            Screen::Home => match home::show(ui, &self.settings, self.error.as_deref()) {
                Some(HomeAction::Open(path)) => self.open(path, &ctx),
                Some(HomeAction::Remove(path)) => {
                    self.settings.remove_recent(&path);
                    self.dirty = true;
                }
                None => {}
            },
            Screen::Loading { path, .. } => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("正在打开 {name} …"));
                        });
                    });
                });
            }
            Screen::Reading(reader) => {
                let action = reader.show(ui, &mut self.settings);
                let key = book_key(&reader.doc.path);
                let loc = reader.location();
                if self.settings.positions.get(&key) != Some(&loc) {
                    self.settings.positions.insert(key, loc);
                    self.dirty = true;
                }
                if let Some(ReaderAction::Close) = action {
                    self.close_book(&ctx);
                }
            }
        }

        let after = self.settings.prefs_fingerprint();
        self.dirty |= before != after;
        if self.dirty && self.last_save.elapsed() >= SAVE_INTERVAL {
            self.settings.save();
            self.dirty = false;
            self.last_save = Instant::now();
        } else if self.dirty {
            ctx.request_repaint_after(SAVE_INTERVAL);
        }
    }

    fn on_exit(&mut self) {
        if let Screen::Reading(r) = &self.screen {
            self.settings
                .positions
                .insert(book_key(&r.doc.path), r.location());
        }
        self.settings.save();
    }
}
