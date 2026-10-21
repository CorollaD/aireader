//! Reflowable text view: one chapter at a time, with off-screen blocks skipped using
//! block heights measured on the previous frame.

use egui::text::LayoutJob;
use egui::{FontFamily, FontId, Frame, Label, Margin, ScrollArea, TextFormat, Ui};

use super::Palette;
use crate::fonts::bold_family;
use crate::model::{Block, Chapter, Location, Span};
use crate::settings::Settings;

const TOP_PAD: f32 = 28.0;
const BOTTOM_PAD: f32 = 48.0;
/// Gap kept above a block scrolled to the top; the visible-block probe must exceed it.
const JUMP_MARGIN: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
pub enum Scroll {
    PageDown,
    PageUp,
    LineDown,
    LineUp,
    Top,
    Bottom,
}

/// Block positions measured for one chapter at one layout configuration.
struct Layout {
    key: (usize, u32, u32, u32),
    /// Content-space y of each block's top; one extra entry for the end.
    tops: Vec<f32>,
}

pub struct FlowView {
    pub chapter: usize,
    /// First block visible at the top of the viewport.
    pub top_block: usize,
    /// (chapter, block) to emphasise, e.g. a search hit.
    pub highlight: Option<(usize, usize)>,
    pending_block: Option<usize>,
    pending_scroll: Option<Scroll>,
    pending_offset: Option<f32>,
    layout: Option<Layout>,
    offset: f32,
    viewport_h: f32,
    content_h: f32,
}

impl FlowView {
    pub fn new(chapter: usize, block: usize) -> Self {
        Self {
            chapter,
            top_block: block,
            highlight: None,
            pending_block: (block > 0).then_some(block),
            pending_scroll: None,
            pending_offset: None,
            layout: None,
            offset: 0.0,
            viewport_h: 0.0,
            content_h: 0.0,
        }
    }

    pub fn location(&self) -> Location {
        Location::Flow {
            chapter: self.chapter,
            block: self.top_block,
        }
    }

    pub fn goto(&mut self, chapter: usize, block: usize) {
        if chapter != self.chapter {
            self.chapter = chapter;
            self.layout = None;
        }
        self.top_block = block;
        self.pending_block = None;
        if block == 0 {
            self.pending_scroll = Some(Scroll::Top);
        } else {
            self.pending_block = Some(block);
        }
    }

    pub fn scroll(&mut self, s: Scroll) {
        self.pending_scroll = Some(s);
    }

    /// Pages forward, moving to the next chapter at the end. Returns false at the end of the book.
    pub fn page_down(&mut self, chapters: usize) -> bool {
        let at_bottom = self.offset + self.viewport_h >= self.content_h - 2.0;
        if !at_bottom {
            self.scroll(Scroll::PageDown);
            true
        } else if self.chapter + 1 < chapters {
            self.goto(self.chapter + 1, 0);
            true
        } else {
            false
        }
    }

    pub fn page_up(&mut self) {
        if self.offset > 1.0 {
            self.scroll(Scroll::PageUp);
        } else if self.chapter > 0 {
            self.goto(self.chapter - 1, 0);
            self.pending_scroll = Some(Scroll::Bottom);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        chapters: &[Chapter],
        settings: &Settings,
        palette: Palette,
    ) {
        let Some(chapter) = chapters.get(self.chapter) else {
            return;
        };
        let avail = ui.available_width();
        let col = (avail - 32.0).min(settings.max_width).max(200.0);
        let margin = ((avail - col) / 2.0).max(0.0);
        let key = (
            self.chapter,
            col.round() as u32,
            (settings.font_size * 10.0) as u32,
            (settings.line_spacing * 100.0) as u32,
        );
        let cached = self
            .layout
            .take()
            .filter(|l| l.key == key && l.tops.len() == chapter.blocks.len() + 1);

        let mut area = ScrollArea::vertical()
            .id_salt(("flow", self.chapter))
            .auto_shrink(false);
        if let Some(s) = self.pending_scroll.take() {
            let page = (self.viewport_h - 3.0 * settings.font_size).max(self.viewport_h * 0.5);
            let line = settings.font_size * settings.line_spacing * 2.0;
            let target = match s {
                Scroll::PageDown => self.offset + page,
                Scroll::PageUp => self.offset - page,
                Scroll::LineDown => self.offset + line,
                Scroll::LineUp => self.offset - line,
                Scroll::Top => 0.0,
                Scroll::Bottom => f32::MAX / 4.0,
            };
            self.pending_offset = Some(target.max(0.0));
        }
        if let Some(off) = self.pending_offset.take() {
            area = area.vertical_scroll_offset(off);
        }

        let highlight = self
            .highlight
            .filter(|(c, _)| *c == self.chapter)
            .map(|(_, b)| b);
        let (has_prev, has_next) = (self.chapter > 0, self.chapter + 1 < chapters.len());
        let mut nav: Option<usize> = None;
        let out = area.show_viewport(ui, |ui, viewport| {
            let origin = ui.min_rect().top();
            let mut tops = Vec::with_capacity(chapter.blocks.len() + 1);
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(col);
                    ui.add_space(TOP_PAD);
                    // Blocks more than one screen away from the viewport are replaced by spacing.
                    let near = viewport.expand2(egui::vec2(0.0, viewport.height()));
                    for (i, block) in chapter.blocks.iter().enumerate() {
                        let y = ui.cursor().top() - origin;
                        tops.push(y);
                        if let Some(c) = &cached
                            && (c.tops[i + 1] < near.top() || c.tops[i] > near.bottom())
                        {
                            ui.add_space(c.tops[i + 1] - c.tops[i]);
                            continue;
                        }
                        if highlight == Some(i) {
                            Frame::NONE
                                .fill(palette.highlight)
                                .corner_radius(4.0)
                                .show(ui, |ui| block_ui(ui, block, settings, palette, col));
                        } else {
                            block_ui(ui, block, settings, palette, col);
                        }
                    }
                    tops.push(ui.cursor().top() - origin);
                    ui.add_space(settings.font_size);
                    ui.horizontal(|ui| {
                        if has_prev && ui.button("← 上一章").clicked() {
                            nav = Some(self.chapter - 1);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if has_next && ui.button("下一章 →").clicked() {
                                nav = Some(self.chapter + 1);
                            }
                        });
                    });
                    ui.add_space(BOTTOM_PAD);
                });
            });
            tops
        });

        let tops = out.inner;
        if let Some(c) = nav {
            self.goto(c, 0);
            return;
        }
        self.offset = out.state.offset.y;
        self.viewport_h = out.inner_rect.height();
        self.content_h = out.content_size.y;
        if self.pending_offset.is_none()
            && self.pending_block.is_none()
            && !chapter.blocks.is_empty()
        {
            let probe = self.offset + JUMP_MARGIN + 4.0;
            self.top_block = tops
                .windows(2)
                .position(|w| w[1] > probe)
                .unwrap_or(chapter.blocks.len() - 1);
        }
        if let Some(b) = self.pending_block.take() {
            match &cached {
                // Heights are known: jump next frame.
                Some(_) => {
                    let y = tops.get(b).copied().unwrap_or(0.0);
                    self.pending_offset = Some((y - JUMP_MARGIN).max(0.0));
                    self.top_block = b;
                }
                None => self.pending_block = Some(b),
            }
            ui.ctx().request_repaint();
        }
        self.layout = Some(Layout { key, tops });
    }
}

fn span_job(
    job: &mut LayoutJob,
    spans: &[Span],
    size: f32,
    base: FontFamily,
    color: egui::Color32,
    palette: Palette,
    s: &Settings,
) {
    for span in spans {
        let family = if span.style.code {
            FontFamily::Monospace
        } else if span.style.bold {
            bold_family()
        } else {
            base.clone()
        };
        let font_size = if span.style.code { size * 0.9 } else { size };
        job.append(
            &span.text,
            0.0,
            TextFormat {
                font_id: FontId::new(font_size, family),
                color: if span.style.link { palette.link } else { color },
                italics: span.style.italic,
                background: if span.style.code {
                    palette.code_bg
                } else {
                    egui::Color32::TRANSPARENT
                },
                line_height: Some(size * s.line_spacing),
                ..Default::default()
            },
        );
    }
}

fn text_job(
    spans: &[Span],
    size: f32,
    family: FontFamily,
    color: egui::Color32,
    palette: Palette,
    s: &Settings,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    span_job(&mut job, spans, size, family, color, palette, s);
    job
}

fn block_ui(ui: &mut Ui, block: &Block, s: &Settings, palette: Palette, col: f32) {
    let size = s.font_size;
    let gap = size * 0.75;
    match block {
        Block::Heading { level, spans } => {
            let scale = match level {
                1 => 1.6,
                2 => 1.4,
                3 => 1.25,
                _ => 1.1,
            };
            ui.add_space(size * 0.6);
            let job = text_job(
                spans,
                size * scale,
                bold_family(),
                palette.strong,
                palette,
                s,
            );
            ui.add(Label::new(job).wrap());
            ui.add_space(gap);
        }
        Block::Paragraph(spans) => {
            ui.add(
                Label::new(text_job(
                    spans,
                    size,
                    FontFamily::Proportional,
                    palette.text,
                    palette,
                    s,
                ))
                .wrap(),
            );
            ui.add_space(gap);
        }
        Block::Quote(spans) => {
            let job = text_job(
                spans,
                size * 0.95,
                FontFamily::Proportional,
                palette.weak,
                palette,
                s,
            );
            Frame::NONE
                .fill(palette.faint_bg)
                .inner_margin(Margin {
                    left: 16,
                    right: 12,
                    top: 6,
                    bottom: 6,
                })
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(Label::new(job).wrap());
                });
            ui.add_space(gap);
        }
        Block::ListItem {
            marker,
            depth,
            spans,
        } => {
            let mut job = LayoutJob::default();
            if !marker.is_empty() {
                span_job(
                    &mut job,
                    &[Span::plain(format!("{marker} "))],
                    size,
                    FontFamily::Proportional,
                    palette.weak,
                    palette,
                    s,
                );
            }
            span_job(
                &mut job,
                spans,
                size,
                FontFamily::Proportional,
                palette.text,
                palette,
                s,
            );
            let indent = (14 + *depth as i32 * 22).min(120) as i8;
            Frame::NONE
                .inner_margin(Margin {
                    left: indent,
                    ..Default::default()
                })
                .show(ui, |ui| {
                    ui.add(Label::new(job).wrap());
                });
            ui.add_space(gap * 0.5);
        }
        Block::Code(code) => {
            Frame::NONE
                .fill(palette.code_bg)
                .inner_margin(Margin::same(10))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let text = egui::RichText::new(code)
                        .monospace()
                        .size(size * 0.85)
                        .color(palette.text);
                    ui.add(Label::new(text).wrap());
                });
            ui.add_space(gap);
        }
        Block::Image(img) => {
            let image = egui::Image::from_bytes(
                img.uri.clone(),
                egui::load::Bytes::Shared(img.bytes.clone()),
            )
            .fit_to_original_size(1.0)
            .max_width(col)
            .max_height(ui.ctx().content_rect().height() * 0.85);
            ui.vertical_centered(|ui| ui.add(image));
            ui.add_space(gap);
        }
        Block::Rule => {
            ui.add_space(gap * 0.5);
            ui.separator();
            ui.add_space(gap * 0.5);
        }
    }
}
