//! `book://` protocol serving binary content to the webview:
//!
//! - `/cover/<book id>` — cover thumbnail (JPEG)
//! - `/font/<file name>` — a reading font the user added
//! - `/img/<book id>/<image id>` — an image embedded in a reflowable book
//! - `/page/<book id>/<page>/<width>[/<rotation>]` — a PDF/CBZ page rendered to PNG (JPEG for
//!   comic pages and scans), `width` pixels wide after an optional clockwise rotation of
//!   90/180/270 degrees
//!
//! Rendered pages are kept in a small in-memory cache, so paging back and forth and window
//! resizes that come back to an earlier width don't render again.

use std::sync::{Arc, Mutex};

use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::commands::AppState;
use crate::formats::percent_decode;
use crate::model::Content;

const MAX_PAGE_WIDTH: u32 = 4096;
/// Total size of the encoded pages kept in memory.
const PAGE_CACHE_BYTES: usize = 128 << 20;
const JPEG_QUALITY: u8 = 85;

/// Encoded pages by path, most recently used last.
struct PageCache {
    entries: Vec<(String, &'static str, Arc<[u8]>)>,
    bytes: usize,
}

static PAGES: Mutex<PageCache> = Mutex::new(PageCache {
    entries: Vec::new(),
    bytes: 0,
});

impl PageCache {
    fn get(&mut self, key: &str) -> Option<(&'static str, Arc<[u8]>)> {
        let i = self.entries.iter().position(|(k, ..)| k == key)?;
        let entry = self.entries.remove(i);
        let found = (entry.1, entry.2.clone());
        self.entries.push(entry);
        Some(found)
    }

    fn put(&mut self, key: String, mime: &'static str, body: Arc<[u8]>) {
        if body.len() > PAGE_CACHE_BYTES / 4 {
            return;
        }
        self.bytes += body.len();
        self.entries.push((key, mime, body));
        while self.bytes > PAGE_CACHE_BYTES && !self.entries.is_empty() {
            let (_, _, old) = self.entries.remove(0);
            self.bytes -= old.len();
        }
    }
}

/// Book ids are the first 16 hex digits of the file's SHA-256.
fn valid_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let path = percent_decode(request.uri().path());
    tauri::async_runtime::spawn_blocking(move || {
        let response = match serve(&app, &path) {
            Ok((mime, body, cache)) => Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, cache)
                .body(body.to_vec()),
            Err(e) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(e.to_string().into_bytes()),
        };
        responder.respond(response.unwrap_or_else(|_| Response::new(Vec::new())));
    });
}

fn mime_of(bytes: &[u8]) -> &'static str {
    match image::guess_format(bytes) {
        Ok(f) => f.to_mime_type(),
        Err(_) if bytes.trim_ascii_start().starts_with(b"<") => "image/svg+xml",
        Err(_) => "application/octet-stream",
    }
}

/// (MIME type, body, Cache-Control).
type Served = (&'static str, Arc<[u8]>, &'static str);

fn serve<R: Runtime>(app: &AppHandle<R>, path: &str) -> anyhow::Result<Served> {
    let state = app.state::<AppState>();
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if let ["font", name] = parts.as_slice() {
        let file = state
            .library()
            .font_path(name)
            .ok_or_else(|| anyhow::anyhow!("no such font"))?;
        let mime = match name
            .rsplit_once('.')
            .map(|(_, e)| e.to_lowercase())
            .as_deref()
        {
            Some("otf") => "font/otf",
            Some("woff") => "font/woff",
            Some("woff2") => "font/woff2",
            _ => "font/ttf",
        };
        return Ok((mime, std::fs::read(file)?.into(), "max-age=604800"));
    }
    if !parts.get(1).is_some_and(|id| valid_id(id)) {
        anyhow::bail!("unknown path: {path}");
    }
    match parts.as_slice() {
        // Cover URLs carry the date the book was added, so a re-added book gets a fresh one.
        ["cover", id] => {
            let file = state.library().cover_path(id);
            Ok(("image/jpeg", std::fs::read(file)?.into(), "max-age=604800"))
        }
        ["img", id, image] => {
            let book = state.open_book(id)?;
            let bytes = book
                .images
                .get(&image.parse()?)
                .ok_or_else(|| anyhow::anyhow!("no such image"))?;
            Ok((mime_of(bytes), bytes.clone(), "max-age=3600"))
        }
        ["page", id, page, width, rest @ ..] => {
            if let Some((mime, body)) = PAGES.lock().unwrap().get(path) {
                return Ok((mime, body, "max-age=3600"));
            }
            let book = state.open_book(id)?;
            let Content::Paged(src) = &book.doc.content else {
                anyhow::bail!("not a paged document")
            };
            let width = width.parse::<u32>()?.clamp(64, MAX_PAGE_WIDTH);
            let rotation = match rest {
                [] => 0,
                [r] => r.parse::<u16>()? % 360,
                _ => anyhow::bail!("unknown path: {path}"),
            };
            let page: usize = page.parse()?;
            let img = render_rotated(src.as_ref(), page, width, rotation)?;
            let (mime, body) = encode(&img, src.photographic(page))?;
            PAGES
                .lock()
                .unwrap()
                .put(path.to_owned(), mime, body.clone());
            Ok((mime, body, "max-age=3600"))
        }
        _ => anyhow::bail!("unknown path: {path}"),
    }
}

/// Encodes a rendered page: JPEG for pictures (much smaller), PNG for pages of text.
fn encode(
    img: &crate::model::PageImage,
    photographic: bool,
) -> anyhow::Result<(&'static str, Arc<[u8]>)> {
    let mut out = Vec::new();
    if photographic {
        let rgb = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone())
                .ok_or_else(|| anyhow::anyhow!("page image has the wrong size"))?,
        )
        .to_rgb8();
        JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).write_image(
            &rgb,
            img.width,
            img.height,
            image::ExtendedColorType::Rgb8,
        )?;
        return Ok(("image/jpeg", out.into()));
    }
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
        .write_image(
            &img.rgba,
            img.width,
            img.height,
            image::ExtendedColorType::Rgba8,
        )?;
    Ok(("image/png", out.into()))
}

/// Renders a page so that, after rotating it clockwise by `rotation` degrees, it is `width`
/// pixels wide.
fn render_rotated(
    src: &dyn crate::model::PageSource,
    page: usize,
    width: u32,
    rotation: u16,
) -> anyhow::Result<crate::model::PageImage> {
    use image::imageops;
    let quarter = rotation == 90 || rotation == 270;
    // On a quarter turn the output width comes from the page's height.
    let render_width = if quarter {
        ((width as f32) / src.page_aspect(page).max(0.01))
            .round()
            .clamp(64.0, MAX_PAGE_WIDTH as f32) as u32
    } else {
        width
    };
    let img = src.render(page, render_width)?;
    if rotation == 0 {
        return Ok(img);
    }
    let buf = image::RgbaImage::from_raw(img.width, img.height, img.rgba)
        .ok_or_else(|| anyhow::anyhow!("page image has the wrong size"))?;
    let rotated = match rotation {
        90 => imageops::rotate90(&buf),
        180 => imageops::rotate180(&buf),
        270 => imageops::rotate270(&buf),
        _ => anyhow::bail!("unsupported rotation: {rotation}"),
    };
    Ok(crate::model::PageImage {
        width: rotated.width(),
        height: rotated.height(),
        rgba: rotated.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PageImage, PageSource};

    /// A 10×20 (portrait) page whose top-left pixel is red.
    struct Fake;
    impl PageSource for Fake {
        fn page_count(&self) -> usize {
            1
        }
        fn page_aspect(&self, _: usize) -> f32 {
            2.0
        }
        fn render(&self, _: usize, width: u32) -> anyhow::Result<PageImage> {
            let mut img =
                image::RgbaImage::from_pixel(width, width * 2, image::Rgba([255, 255, 255, 255]));
            img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
            Ok(PageImage {
                width: img.width(),
                height: img.height(),
                rgba: img.into_raw(),
            })
        }
    }

    #[test]
    fn ids_and_page_cache() {
        assert!(valid_id("0123456789abcdef"));
        assert!(!valid_id(".."));
        assert!(!valid_id("0123456789abcdeg"));
        let mut cache = PageCache {
            entries: Vec::new(),
            bytes: 0,
        };
        let big = PAGE_CACHE_BYTES / 4;
        for i in 0..5 {
            cache.put(format!("p{i}"), "image/png", vec![0u8; big].into());
        }
        assert!(cache.bytes <= PAGE_CACHE_BYTES);
        assert!(cache.get("p0").is_none(), "oldest page is dropped");
        assert!(cache.get("p4").is_some());
    }

    #[test]
    fn photographic_pages_are_jpeg() {
        let img = Fake.render(0, 50).unwrap();
        assert_eq!(encode(&img, true).unwrap().0, "image/jpeg");
        assert_eq!(encode(&img, false).unwrap().0, "image/png");
    }

    #[test]
    fn quarter_turn_keeps_requested_width() {
        let img = render_rotated(&Fake, 0, 200, 90).unwrap();
        assert_eq!((img.width, img.height), (200, 100));
        // Clockwise: the top-left pixel moves to the top-right.
        let rgb = |x: u32, y: u32| {
            let i = ((y * img.width + x) * 4) as usize;
            (img.rgba[i], img.rgba[i + 1], img.rgba[i + 2])
        };
        assert_eq!(rgb(199, 0), (255, 0, 0));
        assert_eq!(rgb(0, 0), (255, 255, 255));
        let img = render_rotated(&Fake, 0, 100, 180).unwrap();
        assert_eq!((img.width, img.height), (100, 200));
    }
}
