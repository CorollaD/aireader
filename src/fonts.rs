//! Loads a system CJK font (egui's bundled fonts have no Chinese glyphs) and a bold face
//! registered as the [`BOLD`] font family.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Font family name used for bold text.
pub const BOLD: &str = "bold";

pub fn bold_family() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

/// (path, face index in a collection).
type Candidate = (&'static str, u32);

#[cfg(target_os = "macos")]
const REGULAR: &[Candidate] = &[
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
    ("/System/Library/Fonts/STHeiti Light.ttc", 1),
    ("/System/Library/Fonts/PingFang.ttc", 0),
    ("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 0),
];
#[cfg(target_os = "macos")]
const BOLD_FACES: &[Candidate] = &[
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 2),
    ("/System/Library/Fonts/STHeiti Medium.ttc", 1),
];

#[cfg(target_os = "windows")]
const REGULAR: &[Candidate] = &[
    ("C:\\Windows\\Fonts\\msyh.ttc", 0),
    ("C:\\Windows\\Fonts\\msyh.ttf", 0),
    ("C:\\Windows\\Fonts\\simhei.ttf", 0),
    ("C:\\Windows\\Fonts\\simsun.ttc", 0),
];
#[cfg(target_os = "windows")]
const BOLD_FACES: &[Candidate] = &[
    ("C:\\Windows\\Fonts\\msyhbd.ttc", 0),
    ("C:\\Windows\\Fonts\\msyhbd.ttf", 0),
];

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const REGULAR: &[Candidate] = &[
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 2),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 2),
    (
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        2,
    ),
    ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
    (
        "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
        0,
    ),
    (
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        0,
    ),
];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const BOLD_FACES: &[Candidate] = &[
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc", 2),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc", 2),
    ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Bold.ttc", 2),
];

fn load_first(candidates: &[Candidate]) -> Option<FontData> {
    candidates.iter().find_map(|(path, index)| {
        let bytes = std::fs::read(path).ok()?;
        let mut data = FontData::from_owned(bytes);
        data.index = *index;
        Some(data)
    })
}

/// Installs fonts. `AIREADER_FONT` may point to a custom `.ttf`/`.otf` used for body text.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let custom = std::env::var("AIREADER_FONT")
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .map(FontData::from_owned);
    let regular = custom.or_else(|| load_first(REGULAR));
    let bold = load_first(BOLD_FACES);

    let mut bold_list = Vec::new();
    if let Some(data) = bold {
        fonts.font_data.insert("cjk-bold".into(), Arc::new(data));
        bold_list.push("cjk-bold".to_owned());
    }
    if let Some(data) = regular {
        fonts.font_data.insert("cjk".into(), Arc::new(data));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".into());
        }
    }
    // Bold falls back to the regular stack for glyphs the bold face lacks.
    bold_list.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts.families.insert(bold_family(), bold_list);
    ctx.set_fonts(fonts);
}
