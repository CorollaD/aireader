//! Fixed-layout page view. Pages are rendered on a worker thread and cached as textures.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Color32, Rect, ScrollArea, TextureHandle, TextureOptions, Ui, pos2, vec2};

use crate::model::PageSource;
use crate::settings::PageLayout;

const GAP: f32 = 12.0;
/// Render widths are rounded up to this step so window resizes reuse textures.
const WIDTH_STEP: u32 = 128;
/// Pages farther than this from the visible range are evicted / not rendered.
const KEEP_RADIUS: usize = 4;

struct Rendered {
    page: usize,
    width: u32,
    image: Result<Option<egui::ColorImage>, String>,
}

pub struct PageView {
    source: Arc<dyn PageSource>,
    pub page: usize,
    pub zoom: f32,
    textures: HashMap<usize, (u32, TextureHandle)>,
    requested: HashSet<(usize, u32)>,
    errors: HashMap<usize, String>,
    requests: Sender<(usize, u32)>,
    results: Receiver<Rendered>,
    /// Visible page range (inclusive), widened by `KEEP_RADIUS`. Shared with the worker,
    /// which skips requests outside it.
    keep: Arc<(AtomicUsize, AtomicUsize)>,
    pending_page: Option<usize>,
    pending_offset: Option<f32>,
    offset: f32,
    viewport_h: f32,
}

impl PageView {
    pub fn new(source: Arc<dyn PageSource>, page: usize, ctx: &egui::Context) -> Self {
        let (req_tx, req_rx) = channel::<(usize, u32)>();
        let (res_tx, res_rx) = channel();
        let keep = Arc::new((
            AtomicUsize::new(page.saturating_sub(KEEP_RADIUS)),
            AtomicUsize::new(page + KEEP_RADIUS),
        ));
        {
            let source = source.clone();
            let keep = keep.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                // Exits when the view (and its sender) is dropped.
                while let Ok(first) = req_rx.recv() {
                    let mut batch = vec![first];
                    batch.extend(req_rx.try_iter());
                    // Newest requests first; skip ones the reader has moved away from.
                    for (page, width) in batch.into_iter().rev() {
                        let (lo, hi) = (
                            keep.0.load(Ordering::Relaxed),
                            keep.1.load(Ordering::Relaxed),
                        );
                        let image = if page < lo || page > hi {
                            Ok(None)
                        } else {
                            source
                                .render(page, width)
                                .map(Some)
                                .map_err(|e| e.to_string())
                        };
                        if res_tx.send(Rendered { page, width, image }).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                }
            });
        }
        let count = source.page_count();
        Self {
            source,
            page: page.min(count.saturating_sub(1)),
            zoom: 1.0,
            textures: HashMap::new(),
            requested: HashSet::new(),
            errors: HashMap::new(),
            requests: req_tx,
            results: res_rx,
            keep,
            pending_page: Some(page),
            pending_offset: None,
            offset: 0.0,
            viewport_h: 0.0,
        }
    }

    pub fn page_count(&self) -> usize {
        self.source.page_count()
    }

    pub fn goto(&mut self, page: usize) {
        let page = page.min(self.page_count().saturating_sub(1));
        self.page = page;
        self.pending_page = Some(page);
    }

    pub fn next(&mut self) {
        self.goto(self.page + 1);
    }

    pub fn prev(&mut self) {
        self.goto(self.page.saturating_sub(1));
    }

    /// Scrolls by a fraction of the viewport (continuous layout).
    pub fn scroll_by(&mut self, fraction: f32) {
        self.pending_offset = Some((self.offset + self.viewport_h * fraction).max(0.0));
    }

    fn receive(&mut self, ctx: &egui::Context) {
        for r in self.results.try_iter() {
            self.requested.remove(&(r.page, r.width));
            match r.image {
                Ok(Some(img)) => {
                    let tex =
                        ctx.load_texture(format!("page-{}", r.page), img, TextureOptions::LINEAR);
                    self.textures.insert(r.page, (r.width, tex));
                    self.errors.remove(&r.page);
                }
                Ok(None) => {}
                Err(e) => {
                    self.errors.insert(r.page, e);
                }
            }
        }
        let (lo, hi) = self.keep_range();
        self.textures.retain(|p, _| (lo..=hi).contains(p));
    }

    fn keep_range(&self) -> (usize, usize) {
        (
            self.keep.0.load(Ordering::Relaxed),
            self.keep.1.load(Ordering::Relaxed),
        )
    }

    fn set_visible(&self, first: usize, last: usize) {
        self.keep
            .0
            .store(first.saturating_sub(KEEP_RADIUS), Ordering::Relaxed);
        self.keep.1.store(last + KEEP_RADIUS, Ordering::Relaxed);
    }

    fn request(&mut self, page: usize, width_px: f32) {
        if self.errors.contains_key(&page) {
            return;
        }
        let width = ((width_px.max(64.0) as u32).div_ceil(WIDTH_STEP) * WIDTH_STEP).min(4096);
        if self.textures.get(&page).is_some_and(|(w, _)| *w == width)
            || self.requested.contains(&(page, width))
        {
            return;
        }
        if self.requests.send((page, width)).is_ok() {
            self.requested.insert((page, width));
        }
    }

    /// Draws page `i` into `rect` (texture if available, otherwise a placeholder).
    fn paint_page(&mut self, ui: &mut Ui, i: usize, rect: Rect) {
        let ppp = ui.ctx().pixels_per_point();
        self.request(i, rect.width() * ppp);
        let painter = ui.painter_at(rect.expand(4.0));
        painter.rect_filled(
            rect.translate(vec2(0.0, 2.0)),
            2.0,
            Color32::from_black_alpha(30),
        );
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        if let Some((_, tex)) = self.textures.get(&i) {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(tex.id(), rect, uv, Color32::WHITE);
        } else if let Some(err) = self.errors.get(&i) {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("渲染失败: {err}"),
                egui::FontId::proportional(14.0),
                Color32::DARK_RED,
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{}", i + 1),
                egui::FontId::proportional(28.0),
                Color32::LIGHT_GRAY,
            );
        }
    }

    pub fn show(&mut self, ui: &mut Ui, layout: PageLayout) {
        self.receive(ui.ctx());
        if self.page_count() == 0 {
            ui.centered_and_justified(|ui| ui.label("没有页面"));
            return;
        }
        match layout {
            PageLayout::Single => self.show_single(ui),
            PageLayout::Continuous => self.show_continuous(ui),
        }
    }

    fn show_single(&mut self, ui: &mut Ui) {
        self.pending_page = None;
        let avail = ui.available_size() - vec2(16.0, 16.0);
        let aspect = self.source.page_aspect(self.page);
        let fit = avail.x.min(avail.y / aspect).max(100.0);
        let size = vec2(fit, fit * aspect) * self.zoom;
        let page = self.page;
        self.set_visible(page, page);
        ScrollArea::both()
            .id_salt(("single", page))
            .auto_shrink(false)
            .show(ui, |ui| {
                let (outer, _) = ui.allocate_exact_size(
                    vec2(size.x.max(avail.x), size.y.max(avail.y)) + vec2(16.0, 16.0),
                    egui::Sense::hover(),
                );
                let rect = Rect::from_center_size(outer.center(), size);
                self.paint_page(ui, page, rect);
            });
        // Prefetch neighbours at the same size.
        let ppp = ui.ctx().pixels_per_point();
        for p in [page + 1, page.wrapping_sub(1)] {
            if p < self.page_count() {
                self.request(p, size.x * ppp);
            }
        }
    }

    fn show_continuous(&mut self, ui: &mut Ui) {
        let count = self.page_count();
        let avail_w = ui.available_width();
        let width = ((avail_w - 32.0).min(1000.0) * self.zoom).max(100.0);
        let mut tops = Vec::with_capacity(count + 1);
        let mut y = GAP;
        for i in 0..count {
            tops.push(y);
            y += width * self.source.page_aspect(i) + GAP;
        }
        tops.push(y);
        let total = y;

        let mut area = ScrollArea::both().id_salt("continuous").auto_shrink(false);
        if let Some(p) = self.pending_page.take() {
            area = area.vertical_scroll_offset(tops[p] - GAP);
        } else if let Some(off) = self.pending_offset.take() {
            area = area.vertical_scroll_offset(off);
        }
        let out = area.show_viewport(ui, |ui, viewport| {
            let content_w = (width + 32.0).max(avail_w);
            ui.set_min_size(vec2(content_w, total));
            let origin = ui.min_rect().min;
            let x = origin.x + (content_w - width) / 2.0;
            let first = tops
                .partition_point(|t| *t < viewport.top())
                .saturating_sub(1);
            let last = tops
                .partition_point(|t| *t <= viewport.bottom())
                .saturating_sub(1)
                .min(count - 1);
            self.set_visible(first, last);
            for i in first..=last.max(first) {
                let h = tops[i + 1] - tops[i] - GAP;
                let rect = Rect::from_min_size(pos2(x, origin.y + tops[i]), vec2(width, h));
                self.paint_page(ui, i, rect);
            }
        });
        self.offset = out.state.offset.y;
        self.viewport_h = out.inner_rect.height();
        let probe = self.offset + self.viewport_h * 0.3;
        self.page = tops
            .partition_point(|t| *t <= probe)
            .saturating_sub(1)
            .min(count - 1);
        let ppp = ui.ctx().pixels_per_point();
        if self.page + 1 < count {
            self.request(self.page + 1, width * ppp);
        }
    }
}
