//! Text recognised from pages with no text layer (scanned PDFs).
//!
//! Recognition runs in the webview (`src/ocr`); the results are kept here, by book and page.
//! [`OcrSource`] lays them over the book, so search, AI, reading aloud and text selection use
//! them exactly like a text layer.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use crate::model::{Content, Document, PageImage, PageSource, TextRun};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ocr_pages (
    book_id TEXT NOT NULL,
    page INTEGER NOT NULL,
    runs TEXT NOT NULL,
    PRIMARY KEY (book_id, page)
);
";

/// The recognition models: PP-OCRv6 small (Apache-2.0), fetched when first needed and checked
/// against these sums.
const MODELS: [(&str, &str); 2] = [
    (
        "PP-OCRv6_small_det",
        "d218f6fbf0f1c23d2161bd6ac7f5eaa6104fa89955c09290497e31008e2618e4",
    ),
    (
        "PP-OCRv6_small_rec",
        "d267ab077a44a0eedb1ea8f8c542d263f211de8e9d7a029bf9fcfff7e5a88fb1",
    ),
];
const MODEL_SITE: &str =
    "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const STALL_TIMEOUT: Duration = Duration::from_secs(60);

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A recognition model's bytes, downloaded into `dir` the first time.
pub async fn model(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let (_, sum) = MODELS
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| anyhow!("unknown model: {name}"))?;
    let dest = dir.join(format!("{name}.tar"));
    if let Ok(bytes) = std::fs::read(&dest)
        && sha256(&bytes) == *sum
    {
        return Ok(bytes);
    }
    std::fs::create_dir_all(dir)?;
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(STALL_TIMEOUT)
        .build()?;
    let url = format!("{MODEL_SITE}/{name}_onnx_infer.tar");
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .context(crate::t!("ocr_download_failed"))?;
    let temp = dest.with_extension("part");
    let mut file = std::fs::File::create(&temp)?;
    while let Some(chunk) = response
        .chunk()
        .await
        .context(crate::t!("ocr_download_failed"))?
    {
        file.write_all(&chunk)?;
    }
    drop(file);
    let bytes = std::fs::read(&temp)?;
    if sha256(&bytes) != *sum {
        let _ = std::fs::remove_file(&temp);
        bail!(crate::t!("ocr_download_failed"));
    }
    std::fs::rename(&temp, &dest)?;
    Ok(bytes)
}

/// Runs far taller than wide would become huge glyphs in the selection layer (see `pdf.rs`).
const TALL_RUN: f32 = 2.0;

pub fn save_page(conn: &Connection, book_id: &str, page: usize, runs: &[TextRun]) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO ocr_pages (book_id, page, runs) VALUES (?1, ?2, ?3)",
        params![book_id, page as i64, serde_json::to_string(runs)?],
    )?;
    Ok(())
}

fn load(conn: &Connection, book_id: &str) -> Result<HashMap<usize, Vec<TextRun>>> {
    let mut stmt = conn.prepare("SELECT page, runs FROM ocr_pages WHERE book_id = ?1")?;
    let rows = stmt.query_map(params![book_id], |r| {
        Ok((r.get::<_, i64>(0)? as usize, r.get::<_, String>(1)?))
    })?;
    let mut pages = HashMap::new();
    for row in rows {
        let (page, runs) = row?;
        pages.insert(page, serde_json::from_str(&runs)?);
    }
    Ok(pages)
}

/// A book's pages with recognised text laid over them.
pub struct OcrSource {
    inner: Arc<dyn PageSource>,
    pages: RwLock<HashMap<usize, Vec<TextRun>>>,
}

impl OcrSource {
    fn new(inner: Arc<dyn PageSource>, pages: HashMap<usize, Vec<TextRun>>) -> Self {
        Self {
            inner,
            pages: RwLock::new(pages),
        }
    }

    pub fn put(&self, page: usize, runs: Vec<TextRun>) {
        self.pages.write().unwrap().insert(page, runs);
    }

    /// Pages with neither a text layer nor recognised text.
    pub fn pending(&self) -> Vec<usize> {
        let done = self.pages.read().unwrap();
        (0..self.inner.page_count())
            .filter(|p| !done.contains_key(p) && self.inner.page_text(*p).is_none())
            .collect()
    }
}

impl PageSource for OcrSource {
    fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    fn page_aspect(&self, index: usize) -> f32 {
        self.inner.page_aspect(index)
    }

    fn render(&self, index: usize, width: u32) -> Result<PageImage> {
        self.inner.render(index, width)
    }

    fn page_text(&self, index: usize) -> Option<String> {
        self.inner.page_text(index).or_else(|| {
            let pages = self.pages.read().unwrap();
            let runs = pages.get(&index).filter(|r| !r.is_empty())?;
            Some(
                runs.iter()
                    .map(|r| r.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })
    }

    fn text_runs(&self, index: usize) -> Vec<TextRun> {
        let own = self.inner.text_runs(index);
        if !own.is_empty() {
            return own;
        }
        let pages = self.pages.read().unwrap();
        pages.get(&index).map_or_else(Vec::new, |runs| {
            runs.iter()
                .filter(|r| r.h <= TALL_RUN * r.w)
                .cloned()
                .collect()
        })
    }

    fn image_dpi(&self, index: usize) -> Option<f32> {
        self.inner.image_dpi(index)
    }

    fn photographic(&self, index: usize) -> bool {
        self.inner.photographic(index)
    }
}

/// Lays a book's recognised text over a paged document.
pub fn attach(
    doc: &mut Document,
    conn: &Connection,
    book_id: &str,
) -> Result<Option<Arc<OcrSource>>> {
    let Content::Paged(src) = &doc.content else {
        return Ok(None);
    };
    let source = Arc::new(OcrSource::new(src.clone(), load(conn, book_id)?));
    doc.content = Content::Paged(source.clone());
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    /// Page 0 has a text layer, page 1 is a scan.
    struct Scan;

    impl PageSource for Scan {
        fn page_count(&self) -> usize {
            2
        }
        fn page_aspect(&self, _: usize) -> f32 {
            1.4
        }
        fn render(&self, _: usize, _: u32) -> Result<PageImage> {
            anyhow::bail!("not used")
        }
        fn page_text(&self, index: usize) -> Option<String> {
            (index == 0).then(|| "原有文字".to_owned())
        }
        fn text_runs(&self, index: usize) -> Vec<TextRun> {
            if index == 0 {
                vec![run("原有文字", 0.2, 0.05)]
            } else {
                Vec::new()
            }
        }
        fn photographic(&self, index: usize) -> bool {
            index == 1
        }
    }

    fn run(text: &str, w: f32, h: f32) -> TextRun {
        TextRun {
            text: text.into(),
            x: 0.1,
            y: 0.1,
            w,
            h,
        }
    }

    fn source(pages: HashMap<usize, Vec<TextRun>>) -> OcrSource {
        OcrSource::new(Arc::new(Scan), pages)
    }

    #[test]
    fn recognised_text_acts_as_a_text_layer() {
        let src = source(HashMap::new());
        assert_eq!(src.pending(), [1], "pages with text need nothing");
        assert_eq!(src.page_text(1), None);

        src.put(1, vec![run("第一行", 0.5, 0.02), run("第二行", 0.5, 0.02)]);
        assert!(src.pending().is_empty());
        assert_eq!(src.page_text(1).as_deref(), Some("第一行\n第二行"));
        assert_eq!(src.text_runs(1).len(), 2);
        assert!(src.photographic(1), "it is still served as a picture");
    }

    #[test]
    fn a_text_layer_is_never_replaced() {
        let src = source(HashMap::from([(0, vec![run("识别", 0.5, 0.02)])]));
        assert_eq!(src.page_text(0).as_deref(), Some("原有文字"));
        assert_eq!(src.text_runs(0)[0].text, "原有文字");
    }

    #[test]
    fn a_blank_page_is_done_but_has_no_text() {
        let src = source(HashMap::from([(1, Vec::new())]));
        assert!(src.pending().is_empty());
        assert_eq!(src.page_text(1), None);
    }

    #[test]
    fn vertical_lines_stay_out_of_the_selection_layer_only() {
        let src = source(HashMap::from([(1, vec![run("竖排的文字", 0.02, 0.5)])]));
        assert!(src.text_runs(1).is_empty());
        assert_eq!(src.page_text(1).as_deref(), Some("竖排的文字"));
    }

    #[tokio::test]
    async fn only_known_models_are_fetched() {
        let dir = std::env::temp_dir().join(format!("aireader-ocr-models-{}", std::process::id()));
        assert!(model(&dir, "../../etc/passwd").await.is_err());
        assert!(!dir.exists(), "nothing was written");
    }

    #[test]
    fn results_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("aireader-ocr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("t.db")).unwrap();
        let conn = db.conn();
        save_page(&conn, "b", 1, &[run("保存", 0.3, 0.02)]).unwrap();
        save_page(&conn, "other", 1, &[run("别的书", 0.3, 0.02)]).unwrap();
        let src = source(load(&conn, "b").unwrap());
        assert_eq!(src.page_text(1).as_deref(), Some("保存"));
        crate::db::delete_book_data(&conn, "b").unwrap();
        assert!(load(&conn, "b").unwrap().is_empty());
        assert_eq!(load(&conn, "other").unwrap().len(), 1);
    }
}
