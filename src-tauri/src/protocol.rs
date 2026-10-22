//! `book://` protocol serving binary content to the webview:
//!
//! - `/cover/<book id>` — cover thumbnail (JPEG)
//! - `/img/<book id>/<image id>` — an image embedded in a reflowable book
//! - `/page/<book id>/<page>/<width>[/<rotation>]` — a PDF/CBZ page rendered to PNG, `width`
//!   pixels wide after an optional clockwise rotation of 90/180/270 degrees

use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::commands::AppState;
use crate::formats::percent_decode;
use crate::model::Content;

const MAX_PAGE_WIDTH: u32 = 4096;

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
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(body),
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
type Served = (&'static str, Vec<u8>, &'static str);

fn serve<R: Runtime>(app: &AppHandle<R>, path: &str) -> anyhow::Result<Served> {
    let state = app.state::<AppState>();
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match parts.as_slice() {
        ["cover", id] => {
            let file = state.library().cover_path(id);
            Ok(("image/jpeg", std::fs::read(file)?, "no-cache"))
        }
        ["img", id, image] => {
            let book = state.open_book(id)?;
            let bytes = book
                .images
                .get(&image.parse()?)
                .ok_or_else(|| anyhow::anyhow!("图片不存在"))?;
            Ok((mime_of(bytes), bytes.to_vec(), "max-age=3600"))
        }
        ["page", id, page, width, rest @ ..] => {
            let book = state.open_book(id)?;
            let Content::Paged(src) = &book.doc.content else {
                anyhow::bail!("不是分页文档")
            };
            let width = width.parse::<u32>()?.clamp(64, MAX_PAGE_WIDTH);
            let rotation = match rest {
                [] => 0,
                [r] => r.parse::<u16>()? % 360,
                _ => anyhow::bail!("未知路径: {path}"),
            };
            let img = render_rotated(src.as_ref(), page.parse()?, width, rotation)?;
            let mut png = Vec::new();
            PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
                .write_image(
                    &img.rgba,
                    img.width,
                    img.height,
                    image::ExtendedColorType::Rgba8,
                )?;
            Ok(("image/png", png, "max-age=3600"))
        }
        _ => anyhow::bail!("未知路径: {path}"),
    }
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
        .ok_or_else(|| anyhow::anyhow!("页面图像尺寸不符"))?;
    let rotated = match rotation {
        90 => imageops::rotate90(&buf),
        180 => imageops::rotate180(&buf),
        270 => imageops::rotate270(&buf),
        _ => anyhow::bail!("不支持的旋转角度: {rotation}"),
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
