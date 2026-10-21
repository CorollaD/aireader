pub mod flow_view;
pub mod home;
pub mod page_view;
pub mod reader;

use egui::{Color32, Visuals};

use crate::settings::Theme;

pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let mut v = match theme {
        Theme::Dark => Visuals::dark(),
        Theme::Light | Theme::Sepia => Visuals::light(),
    };
    if theme == Theme::Sepia {
        let paper = Color32::from_rgb(244, 236, 216);
        v.panel_fill = paper;
        v.window_fill = paper;
        v.extreme_bg_color = Color32::from_rgb(250, 245, 232);
        v.faint_bg_color = Color32::from_rgb(234, 224, 200);
        v.code_bg_color = Color32::from_rgb(232, 221, 196);
        v.override_text_color = Some(Color32::from_rgb(78, 60, 42));
        v.widgets.noninteractive.bg_stroke.color = Color32::from_rgb(214, 200, 170);
    }
    ctx.set_theme(if theme == Theme::Dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    ctx.set_visuals(v);
}

/// Colours used when rendering book content.
#[derive(Clone, Copy)]
pub struct Palette {
    pub text: Color32,
    pub strong: Color32,
    pub weak: Color32,
    pub link: Color32,
    pub code_bg: Color32,
    pub faint_bg: Color32,
    pub highlight: Color32,
}

impl Palette {
    pub fn from_visuals(v: &Visuals) -> Self {
        Self {
            // Body text a little stronger than egui's UI text colour, for long reading.
            text: v.override_text_color.unwrap_or(if v.dark_mode {
                Color32::from_gray(212)
            } else {
                Color32::from_gray(34)
            }),
            strong: v.strong_text_color(),
            weak: v.weak_text_color(),
            link: v.hyperlink_color,
            code_bg: v.code_bg_color,
            faint_bg: v.faint_bg_color,
            highlight: v.selection.bg_fill.gamma_multiply(0.45),
        }
    }
}
