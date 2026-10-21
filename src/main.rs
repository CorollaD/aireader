#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AiReader")
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([480.0, 360.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "AiReader",
        options,
        Box::new(|cc| Ok(Box::new(aireader::app::App::new(cc, initial)))),
    )
}
