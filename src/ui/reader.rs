//! Reader screen: toolbar, table of contents, search and the content view.

use egui::{Align, Key, Layout, RichText, ScrollArea, Ui};

use super::Palette;
use super::flow_view::{FlowView, Scroll};
use super::page_view::PageView;
use crate::model::{Content, Document, Location};
use crate::settings::{PageLayout, Settings, Theme};

const MAX_HITS: usize = 1000;

pub enum ReaderAction {
    Close,
}

enum View {
    Flow(FlowView),
    Paged(PageView),
}

struct Hit {
    chapter: usize,
    block: usize,
    snippet: String,
}

#[derive(Default)]
struct Search {
    open: bool,
    focus: bool,
    query: String,
    searched: String,
    hits: Vec<Hit>,
}

pub struct Reader {
    pub doc: Document,
    view: View,
    search: Search,
    /// Scroll the TOC so the current entry is visible (set after navigation).
    reveal_toc: bool,
}

impl Reader {
    pub fn new(doc: Document, saved: Option<Location>, ctx: &egui::Context) -> Self {
        let view = match &doc.content {
            Content::Flow(chapters) => {
                let (c, b) = match saved {
                    Some(Location::Flow { chapter, block }) if chapter < chapters.len() => {
                        (chapter, block)
                    }
                    _ => (0, 0),
                };
                View::Flow(FlowView::new(c, b))
            }
            Content::Paged(source) => {
                let page = match saved {
                    Some(Location::Page(p)) => p,
                    _ => 0,
                };
                View::Paged(PageView::new(source.clone(), page, ctx))
            }
        };
        Self {
            doc,
            view,
            search: Search::default(),
            reveal_toc: true,
        }
    }

    pub fn location(&self) -> Location {
        match &self.view {
            View::Flow(f) => f.location(),
            View::Paged(p) => Location::Page(p.page),
        }
    }

    fn goto(&mut self, loc: Location) {
        match (&mut self.view, loc) {
            (View::Flow(f), Location::Flow { chapter, block }) => f.goto(chapter, block),
            (View::Paged(p), Location::Page(page)) => p.goto(page),
            _ => {}
        }
    }

    fn chapter_count(&self) -> usize {
        match &self.doc.content {
            Content::Flow(c) => c.len(),
            Content::Paged(_) => 0,
        }
    }

    /// Index of the last TOC entry at or before the current position.
    fn current_toc(&self) -> Option<usize> {
        let key = |l: Location| match l {
            Location::Flow { chapter, block } => (chapter, block),
            Location::Page(p) => (p, 0),
        };
        let here = key(self.location());
        self.doc.toc.iter().rposition(|e| key(e.location) <= here)
    }

    pub fn show(&mut self, ui: &mut Ui, settings: &mut Settings) -> Option<ReaderAction> {
        let mut action = None;
        self.handle_keys(ui.ctx(), settings);

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("← 书架").on_hover_text("返回书架").clicked() {
                    action = Some(ReaderAction::Close);
                }
                if !self.doc.toc.is_empty() {
                    ui.toggle_value(&mut settings.show_toc, "☰ 目录")
                        .on_hover_text("目录 (T)");
                }
                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    self.toolbar_right(ui, settings);
                    ui.add(egui::Label::new(RichText::new(&self.doc.title).strong()).truncate());
                });
            });
            ui.add_space(2.0);
        });

        if settings.show_toc && !self.doc.toc.is_empty() {
            egui::Panel::left("toc")
                .resizable(true)
                .default_size(260.0)
                .show(ui, |ui| self.toc_ui(ui));
        }
        if self.search.open && matches!(self.view, View::Flow(_)) {
            egui::Panel::right("search")
                .resizable(true)
                .default_size(300.0)
                .show(ui, |ui| self.search_ui(ui));
        }

        let palette = Palette::from_visuals(ui.visuals());
        egui::CentralPanel::default().show(ui, |ui| match &mut self.view {
            View::Flow(f) => {
                if let Content::Flow(chapters) = &self.doc.content {
                    f.show(ui, chapters, settings, palette);
                }
            }
            View::Paged(p) => p.show(ui, settings.page_layout),
        });
        action
    }

    fn toolbar_right(&mut self, ui: &mut Ui, settings: &mut Settings) {
        egui::ComboBox::from_id_salt("theme")
            .selected_text(settings.theme.label())
            .width(64.0)
            .show_ui(ui, |ui| {
                for t in Theme::ALL {
                    ui.selectable_value(&mut settings.theme, t, t.label());
                }
            });
        let chapters = self.chapter_count();
        match &mut self.view {
            View::Flow(f) => {
                ui.menu_button("Aa 排版", |ui| {
                    ui.add(egui::Slider::new(&mut settings.font_size, 12.0..=40.0).text("字号"));
                    ui.add(egui::Slider::new(&mut settings.line_spacing, 1.2..=2.6).text("行距"));
                    ui.add(
                        egui::Slider::new(&mut settings.max_width, 400.0..=1600.0).text("版心宽度"),
                    );
                });
                if ui
                    .selectable_label(self.search.open, "🔍")
                    .on_hover_text("搜索 (Ctrl/⌘+F)")
                    .clicked()
                {
                    self.search.open = !self.search.open;
                    self.search.focus = self.search.open;
                }
                ui.label(format!("{} / {}", f.chapter + 1, chapters));
            }
            View::Paged(p) => {
                ui.selectable_value(&mut settings.page_layout, PageLayout::Single, "单页");
                ui.selectable_value(&mut settings.page_layout, PageLayout::Continuous, "连续");
                ui.separator();
                if ui.button("＋").on_hover_text("放大 (Ctrl/⌘+=)").clicked() {
                    p.zoom = (p.zoom * 1.2).min(5.0);
                }
                ui.label(format!("{:.0}%", p.zoom * 100.0));
                if ui.button("－").on_hover_text("缩小 (Ctrl/⌘+-)").clicked() {
                    p.zoom = (p.zoom / 1.2).max(0.2);
                }
                ui.separator();
                let count = p.page_count();
                ui.label(format!("/ {count}"));
                let mut page = p.page + 1;
                if ui
                    .add(egui::DragValue::new(&mut page).range(1..=count.max(1)))
                    .changed()
                {
                    p.goto(page - 1);
                }
            }
        }
    }

    fn toc_ui(&mut self, ui: &mut Ui) {
        let current = self.current_toc();
        let mut target = None;
        let reveal = std::mem::take(&mut self.reveal_toc);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, entry) in self.doc.toc.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(entry.depth as f32 * 14.0);
                    let resp = ui.selectable_label(current == Some(i), &entry.title);
                    if resp.clicked() {
                        target = Some(entry.location);
                    }
                    if reveal && current == Some(i) {
                        resp.scroll_to_me(Some(Align::Center));
                    }
                });
            }
        });
        if let Some(loc) = target {
            self.goto(loc);
        }
    }

    fn search_ui(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut self.search.query)
                .hint_text("搜索全文，回车确认")
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.search.focus) {
            resp.request_focus();
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.run_search();
        }
        if !self.search.searched.is_empty() {
            let n = self.search.hits.len();
            let more = if n >= MAX_HITS { "+" } else { "" };
            ui.label(RichText::new(format!("找到 {n}{more} 处")).weak());
        }
        ui.separator();
        let mut target = None;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let chapters = match &self.doc.content {
                Content::Flow(c) => c.as_slice(),
                Content::Paged(_) => &[],
            };
            for hit in &self.search.hits {
                let title = chapters
                    .get(hit.chapter)
                    .map(|c| c.title.as_str())
                    .unwrap_or("");
                let text = format!("{}\n{}", title, hit.snippet);
                if ui
                    .add(egui::Button::new(text).wrap().frame(false))
                    .clicked()
                {
                    target = Some((hit.chapter, hit.block));
                }
                ui.separator();
            }
        });
        if let (Some((c, b)), View::Flow(f)) = (target, &mut self.view) {
            f.goto(c, b);
            f.highlight = Some((c, b));
        }
    }

    fn run_search(&mut self) {
        let query = self.search.query.trim().to_lowercase();
        self.search.hits.clear();
        self.search.searched = query.clone();
        let Content::Flow(chapters) = &self.doc.content else {
            return;
        };
        if query.is_empty() {
            return;
        }
        'outer: for (ci, ch) in chapters.iter().enumerate() {
            for (bi, block) in ch.blocks.iter().enumerate() {
                let text = block.text();
                let lower = text.to_lowercase();
                let Some(pos) = lower.find(&query) else {
                    continue;
                };
                // Byte offsets only line up when lowercasing kept the length.
                let src = if lower.len() == text.len() {
                    &text
                } else {
                    &lower
                };
                let start = src[..pos]
                    .char_indices()
                    .rev()
                    .nth(24)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let end = src[pos..]
                    .char_indices()
                    .nth(query.chars().count() + 40)
                    .map(|(i, _)| pos + i)
                    .unwrap_or(src.len());
                let snippet = format!(
                    "{}{}{}",
                    if start > 0 { "…" } else { "" },
                    src[start..end].replace('\n', " "),
                    if end < src.len() { "…" } else { "" }
                );
                self.search.hits.push(Hit {
                    chapter: ci,
                    block: bi,
                    snippet,
                });
                if self.search.hits.len() >= MAX_HITS {
                    break 'outer;
                }
            }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context, settings: &mut Settings) {
        let command = ctx.input(|i| i.modifiers.command);
        if command && ctx.input(|i| i.key_pressed(Key::F)) && matches!(self.view, View::Flow(_)) {
            self.search.open = true;
            self.search.focus = true;
        }
        if ctx.egui_wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.search.open = false;
            }
            return;
        }
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        let shift = ctx.input(|i| i.modifiers.shift);
        let zoom_in = command && (pressed(Key::Equals) || pressed(Key::Plus));
        let zoom_out = command && pressed(Key::Minus);
        if !command && pressed(Key::T) {
            settings.show_toc = !settings.show_toc;
        }
        if pressed(Key::Escape) {
            self.search.open = false;
        }
        let chapters = self.chapter_count();
        let before = self.location();
        match &mut self.view {
            View::Flow(f) => {
                if zoom_in {
                    settings.font_size = (settings.font_size + 1.0).min(40.0);
                }
                if zoom_out {
                    settings.font_size = (settings.font_size - 1.0).max(12.0);
                }
                if pressed(Key::ArrowRight) && f.chapter + 1 < chapters {
                    f.goto(f.chapter + 1, 0);
                }
                if pressed(Key::ArrowLeft) && f.chapter > 0 {
                    f.goto(f.chapter - 1, 0);
                }
                if pressed(Key::PageDown) || (pressed(Key::Space) && !shift) {
                    f.page_down(chapters);
                }
                if pressed(Key::PageUp) || (pressed(Key::Space) && shift) {
                    f.page_up();
                }
                if pressed(Key::ArrowDown) {
                    f.scroll(Scroll::LineDown);
                }
                if pressed(Key::ArrowUp) {
                    f.scroll(Scroll::LineUp);
                }
                if pressed(Key::Home) {
                    f.scroll(Scroll::Top);
                }
                if pressed(Key::End) {
                    f.scroll(Scroll::Bottom);
                }
            }
            View::Paged(p) => {
                if zoom_in {
                    p.zoom = (p.zoom * 1.2).min(5.0);
                }
                if zoom_out {
                    p.zoom = (p.zoom / 1.2).max(0.2);
                }
                let single = settings.page_layout == PageLayout::Single;
                if pressed(Key::ArrowRight)
                    || (single
                        && (pressed(Key::PageDown)
                            || pressed(Key::Space)
                            || pressed(Key::ArrowDown)))
                {
                    p.next();
                }
                if pressed(Key::ArrowLeft)
                    || (single && (pressed(Key::PageUp) || pressed(Key::ArrowUp)))
                {
                    p.prev();
                }
                if !single {
                    if pressed(Key::PageDown) || (pressed(Key::Space) && !shift) {
                        p.scroll_by(0.9);
                    }
                    if pressed(Key::PageUp) || (pressed(Key::Space) && shift) {
                        p.scroll_by(-0.9);
                    }
                }
                if pressed(Key::Home) {
                    p.goto(0);
                }
                if pressed(Key::End) {
                    p.goto(usize::MAX);
                }
            }
        }
        if self.location() != before {
            self.reveal_toc = true;
        }
    }
}
