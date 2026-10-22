//! Comic book archives (`.cbz`): images in a zip, ordered naturally by file name.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use image::imageops::FilterType;

use crate::model::{Content, Document, PageImage, PageSource};

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

pub struct CbzSource {
    zip: Mutex<zip::ZipArchive<File>>,
    names: Vec<String>,
    /// Aspect ratios, filled in as pages are decoded.
    aspects: Mutex<Vec<Option<f32>>>,
}

impl CbzSource {
    fn read(&self, index: usize) -> Result<Vec<u8>> {
        let mut zip = self.zip.lock().unwrap();
        let mut entry = zip.by_name(&self.names[index])?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

impl PageSource for CbzSource {
    fn page_count(&self) -> usize {
        self.names.len()
    }

    fn page_aspect(&self, index: usize) -> f32 {
        let known = self.aspects.lock().unwrap().get(index).copied().flatten();
        known.unwrap_or(1.5)
    }

    fn render(&self, index: usize, width: u32) -> Result<PageImage> {
        let img = image::load_from_memory(&self.read(index)?)?;
        if img.width() > 0 {
            self.aspects.lock().unwrap()[index] = Some(img.height() as f32 / img.width() as f32);
        }
        let img = if img.width() > width {
            let h = (img.height() as f32 * width as f32 / img.width() as f32)
                .round()
                .max(1.0) as u32;
            img.resize_exact(width, h, FilterType::Triangle)
        } else {
            img
        };
        let rgba = img.to_rgba8();
        Ok(PageImage {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    }
}

pub fn open_source(path: &Path) -> Result<CbzSource> {
    let zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .filter(|n| {
            !n.starts_with("__MACOSX") && !n.rsplit('/').next().unwrap_or("").starts_with('.')
        })
        .filter(|n| {
            let ext = n
                .rsplit_once('.')
                .map(|(_, e)| e.to_lowercase())
                .unwrap_or_default();
            IMAGE_EXTS.contains(&ext.as_str())
        })
        .map(|n| n.into_owned())
        .collect();
    if names.is_empty() {
        bail!(crate::t!("archive_no_images"));
    }
    names.sort_by(|a, b| natord::compare_ignore_case(a, b));
    let aspects = Mutex::new(vec![None; names.len()]);
    Ok(CbzSource {
        zip: Mutex::new(zip),
        names,
        aspects,
    })
}

pub fn load(path: &Path) -> Result<Document> {
    let source = open_source(path)?;
    Ok(Document {
        path: path.to_owned(),
        title: Document::file_stem(path),
        author: None,
        toc: Vec::new(),
        content: Content::Paged(Arc::new(source)),
    })
}
