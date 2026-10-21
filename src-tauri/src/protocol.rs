//! `book://` protocol serving binary content to the webview:
//!
//! - `/cover/<book id>` — cover thumbnail (JPEG)
//! - `/img/<book id>/<image id>` — an image embedded in a reflowable book
//! - `/page/<book id>/<page>/<width>` — a PDF/CBZ page rendered to PNG

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
        ["page", id, page, width] => {
            let book = state.open_book(id)?;
            let Content::Paged(src) = &book.doc.content else {
                anyhow::bail!("不是分页文档")
            };
            let width = width.parse::<u32>()?.clamp(64, MAX_PAGE_WIDTH);
            let img = src.render(page.parse()?, width)?;
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
