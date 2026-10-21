//! Start screen: open a file, drag & drop, recently opened books.

use std::path::PathBuf;

use egui::{Align, Layout, RichText, ScrollArea, Ui};

use crate::formats::EXTENSIONS;
use crate::settings::Settings;

pub enum HomeAction {
    Open(PathBuf),
    Remove(PathBuf),
}

pub fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("电子书与文档", EXTENSIONS)
        .add_filter("所有文件", &["*"])
        .pick_file()
}

pub fn show(ui: &mut Ui, settings: &Settings, error: Option<&str>) -> Option<HomeAction> {
    let mut action = None;
    egui::CentralPanel::default().show(ui, |ui| {
        let width = ui.available_width().min(720.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(width);
            ui.add_space(48.0);
            ui.label(RichText::new("AiReader").size(34.0).strong());
            ui.label(
                RichText::new("EPUB · PDF · TXT · Markdown · MOBI/AZW3 · FB2 · DOCX · CBZ").weak(),
            );
            ui.add_space(20.0);
            if ui
                .add(
                    egui::Button::new(RichText::new("打开文件…").size(17.0))
                        .min_size(egui::vec2(180.0, 38.0)),
                )
                .clicked()
                && let Some(p) = pick_file()
            {
                action = Some(HomeAction::Open(p));
            }
            ui.add_space(6.0);
            ui.label(RichText::new("也可以把文件拖到窗口中").weak());
            if let Some(err) = error {
                ui.add_space(12.0);
                ui.colored_label(ui.visuals().error_fg_color, err);
            }
            ui.add_space(28.0);
        });

        if settings.recent.is_empty() {
            return;
        }
        let full = ui.available_rect_before_wrap();
        let column = egui::Rect::from_min_size(
            egui::pos2(full.center().x - width / 2.0, full.top()),
            egui::vec2(width, full.height()),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
            ui.label(RichText::new("最近阅读").size(18.0).strong());
            ui.separator();
            ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for r in &settings.recent {
                        let missing = !r.path.exists();
                        ui.horizontal(|ui| {
                            let title = if missing {
                                format!("{}（文件不存在）", r.title)
                            } else {
                                r.title.clone()
                            };
                            let resp = ui
                                .add_enabled(
                                    !missing,
                                    egui::Button::new(RichText::new(title).size(16.0)).frame(false),
                                )
                                .on_hover_text(r.path.display().to_string());
                            if resp.clicked() {
                                action = Some(HomeAction::Open(r.path.clone()));
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("×").on_hover_text("从列表移除").clicked()
                                {
                                    action = Some(HomeAction::Remove(r.path.clone()));
                                }
                                let ext = r
                                    .path
                                    .extension()
                                    .map(|e| e.to_string_lossy().to_uppercase())
                                    .unwrap_or_default();
                                ui.label(RichText::new(ext).weak().small());
                            });
                        });
                    }
                });
        });
    });
    action
}
