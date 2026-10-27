//! Semantic search over a book: its passages embedded with the configured embedding model and
//! stored with rig-sqlite (sqlite-vec) in the library database. An index is built only when
//! the reader asks for it, and is dropped when the embedding model changes.

use anyhow::{Result, anyhow, bail};
use rig_core::DynModel;
use rig_core::operation::Embedding as EmbeddingOp;
use rig_core::vector_store::VectorStoreIndex;
use rig_core::vector_store::request::{SearchFilter, VectorSearchRequest};
use rig_sqlite::{
    Column, ColumnValue, SqliteSearchFilter, SqliteVectorStore, SqliteVectorStoreTable,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::retrieval::Passage;
use crate::db::Db;

const TABLE: &str = "passages";
/// Texts sent per embedding request.
const BATCH: usize = 32;

/// A passage as stored in the vector table. `idx` is its position in the book's passage list
/// ([`super::retrieval::passages`]), which keyword search uses too.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PassageRow {
    pub id: String,
    pub book_id: String,
    pub idx: i64,
    pub text: String,
}

impl SqliteVectorStoreTable for PassageRow {
    fn name() -> &'static str {
        TABLE
    }

    fn schema() -> Vec<Column> {
        vec![
            Column::new("id", "TEXT PRIMARY KEY"),
            Column::new("book_id", "TEXT").indexed(),
            Column::new("idx", "INTEGER"),
            Column::new("text", "TEXT"),
        ]
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn column_values(&self) -> Vec<(&'static str, Box<dyn ColumnValue>)> {
        vec![
            ("id", Box::new(self.id.clone())),
            ("book_id", Box::new(self.book_id.clone())),
            ("idx", Box::new(self.idx)),
            ("text", Box::new(self.text.clone())),
        ]
    }
}

/// Which model the vectors in the database come from (stored in `kv` as `semantic_model`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct IndexModel {
    key: String,
    ndims: usize,
}

/// A book's index state, for the reader's AI panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Passages indexed with the current embedding model; 0 = not built.
    pub passages: usize,
    /// An index exists but was built with another model.
    pub stale: bool,
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE name = ?1",
            params![name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Removes a book's passages and their vectors.
pub fn delete_book_passages(conn: &Connection, book_id: &str) -> Result<()> {
    if !table_exists(conn, TABLE)? {
        return Ok(());
    }
    conn.execute(
        &format!(
            "DELETE FROM {TABLE}_embeddings WHERE rowid IN (
               SELECT m.embedding_rowid FROM {TABLE}_embedding_map m JOIN {TABLE} p ON p.rowid = m.document_rowid
               WHERE p.book_id = ?1)"
        ),
        params![book_id],
    )?;
    conn.execute(
        &format!(
            "DELETE FROM {TABLE}_embedding_map WHERE document_rowid IN (SELECT rowid FROM {TABLE} WHERE book_id = ?1)"
        ),
        params![book_id],
    )?;
    conn.execute(
        &format!("DELETE FROM {TABLE} WHERE book_id = ?1"),
        params![book_id],
    )?;
    conn.execute(
        "DELETE FROM semantic_books WHERE book_id = ?1",
        params![book_id],
    )?;
    Ok(())
}

/// Drops every vector (the model changed: old vectors can't be compared with new ones).
fn drop_all(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS {TABLE}_embeddings; DROP TABLE IF EXISTS {TABLE}_embedding_map;
         DROP TABLE IF EXISTS {TABLE}; DELETE FROM semantic_books;"
    ))?;
    conn.execute("DELETE FROM kv WHERE key = 'semantic_model'", [])?;
    Ok(())
}

pub fn status(db: &Db, book_id: &str, model_key: Option<&str>) -> Result<Status> {
    let conn = db.conn();
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT model, passages FROM semantic_books WHERE book_id = ?1",
            params![book_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(match row {
        Some((model, n)) if Some(model.as_str()) == model_key => Status {
            passages: n as usize,
            stale: false,
        },
        Some(_) => Status {
            passages: 0,
            stale: true,
        },
        None => Status::default(),
    })
}

async fn vector_store(db: &Db, ndims: usize) -> Result<SqliteVectorStore<PassageRow>> {
    crate::db::register_sqlite_vec();
    let conn = tokio_rusqlite::Connection::open(db.path().to_owned()).await?;
    Ok(SqliteVectorStore::new(conn, ndims).await?)
}

/// Embeds `texts` in one request.
async fn embed(
    model: &DynModel<EmbeddingOp>,
    texts: Vec<String>,
) -> Result<Vec<rig_core::embeddings::Embedding>> {
    let n = texts.len();
    let response = model.call(texts).await.map_err(|e| anyhow!("{e}"))?;
    if response.embeddings.len() != n {
        bail!(
            "the embedding service returned {} vectors for {n} texts",
            response.embeddings.len()
        );
    }
    Ok(response.embeddings)
}

/// Builds (or rebuilds) a book's index with `model`, reporting `(done, total)` passages.
pub async fn build(
    db: &Db,
    book_id: &str,
    passages: &[Passage],
    model: &DynModel<EmbeddingOp>,
    model_key: &str,
    progress: &(dyn Fn(usize, usize) + Send + Sync),
) -> Result<usize> {
    if passages.is_empty() {
        bail!(crate::t!("ai_book_no_text"));
    }
    // The vector width, from the model itself (catalogs don't know every local model).
    let ndims = embed(model, vec!["probe".into()])
        .await?
        .first()
        .map(|e| e.vec.len())
        .unwrap_or(0);
    if ndims == 0 {
        bail!("the embedding service returned an empty vector");
    }
    let wanted = IndexModel {
        key: model_key.to_owned(),
        ndims,
    };
    {
        let conn = db.conn();
        let current: Option<IndexModel> = crate::db::kv_get(&conn, "semantic_model")?;
        if current.as_ref() != Some(&wanted) {
            drop_all(&conn)?;
            crate::db::kv_set(&conn, "semantic_model", &wanted)?;
        }
        delete_book_passages(&conn, book_id)?;
    }
    let store = vector_store(db, ndims).await?;
    progress(0, passages.len());
    for (start, chunk) in passages
        .chunks(BATCH)
        .enumerate()
        .map(|(i, c)| (i * BATCH, c))
    {
        let texts: Vec<String> = chunk
            .iter()
            .map(|p| format!("{}\n{}", p.title, p.text))
            .collect();
        let vectors = embed(model, texts).await?;
        let rows = chunk
            .iter()
            .zip(vectors)
            .enumerate()
            .map(|(k, (p, v))| {
                let idx = (start + k) as i64;
                let row = PassageRow {
                    id: format!("{book_id}:{idx}"),
                    book_id: book_id.to_owned(),
                    idx,
                    text: p.text.clone(),
                };
                (row, vec![v])
            })
            .collect();
        store.add_rows(rows).await?;
        progress((start + chunk.len()).min(passages.len()), passages.len());
    }
    db.conn().execute(
        "INSERT OR REPLACE INTO semantic_books (book_id, model, passages, updated) VALUES (?1, ?2, ?3, ?4)",
        params![book_id, model_key, passages.len() as i64, crate::library::now() as i64],
    )?;
    Ok(passages.len())
}

/// The book's passages most similar in meaning to `query`, best first (passage indices).
/// Empty when the book has no index for this model.
pub async fn search(
    db: &Db,
    book_id: &str,
    query: &str,
    model: &DynModel<EmbeddingOp>,
    model_key: &str,
    limit: usize,
) -> Result<Vec<usize>> {
    if status(db, book_id, Some(model_key))?.passages == 0 {
        return Ok(Vec::new());
    }
    let Some(IndexModel { ndims, .. }) =
        crate::db::kv_get::<IndexModel>(&db.conn(), "semantic_model")?
    else {
        return Ok(Vec::new());
    };
    let index = vector_store(db, ndims).await?.index(model.clone());
    let request = VectorSearchRequest::builder()
        .query(query)
        .samples(limit as u64)
        .filter(SqliteSearchFilter::eq(
            "book_id",
            serde_json::json!(book_id),
        ))
        .build();
    let hits = index.top_n_ids(request).await?;
    Ok(hits
        .into_iter()
        .filter_map(|h| h.id.rsplit_once(':').and_then(|(_, i)| i.parse().ok()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Location;

    fn passage(text: &str) -> Passage {
        Passage {
            title: "第一回".into(),
            location: Location::Flow {
                chapter: 0,
                block: 0,
            },
            text: text.into(),
        }
    }

    /// A tiny embedding model: one dimension per keyword, so similarity is predictable.
    #[derive(Clone, Copy)]
    struct Keywords;

    impl rig_core::driver::Transport<rig_core::driver::Local<EmbeddingOp>> for Keywords {
        fn send(
            &self,
            texts: Vec<String>,
            _exchange: rig_core::driver::Exchange,
        ) -> rig_core::driver::Opening<rig_core::driver::Step<EmbeddingOp>> {
            use rig_core::embeddings::{Embedding, EmbeddingResponse};
            let embeddings = texts
                .into_iter()
                .map(|t| Embedding {
                    vec: ["桃园", "曹操", "赤壁", "probe"]
                        .iter()
                        .map(|k| if t.contains(k) { 1.0 } else { 0.01 })
                        .collect(),
                    document: t,
                })
                .collect();
            let response = EmbeddingResponse {
                provider: "mock".into(),
                ..EmbeddingResponse::new(embeddings)
            };
            rig_core::driver::Opening::ready(rig_core::driver::Opened::new(
                futures_util::stream::iter([Ok::<_, rig_core::error::ProviderError>(
                    rig_core::driver::Step::End(response),
                )]),
            ))
        }
    }

    fn keyword_model() -> DynModel<EmbeddingOp> {
        rig_core::driver::Model::new(
            rig_core::driver::Local::new("mock")
                .with_capabilities(rig_core::wire::Capabilities::embedding(32, 4)),
            Keywords,
        )
        .erase()
    }

    #[tokio::test]
    async fn builds_searches_and_resets_on_model_change() {
        let dir = std::env::temp_dir().join(format!("aireader-sem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("t.db")).unwrap();
        let model = keyword_model();
        let book = vec![
            passage("刘备桃园结义"),
            passage("曹操煮酒论英雄"),
            passage("赤壁之战火烧连营"),
        ];
        let other = vec![passage("曹操的另一本书")];
        let calls = std::sync::Mutex::new(Vec::new());
        let n = build(&db, "b1", &book, &model, "mock/kw", &|d, t| {
            calls.lock().unwrap().push((d, t))
        })
        .await
        .unwrap();
        assert_eq!(n, 3);
        assert_eq!(calls.into_inner().unwrap().last(), Some(&(3, 3)));
        build(&db, "b2", &other, &model, "mock/kw", &|_, _| {})
            .await
            .unwrap();
        assert_eq!(
            status(&db, "b1", Some("mock/kw")).unwrap(),
            Status {
                passages: 3,
                stale: false
            }
        );
        // Only the asked-about book is searched.
        let hits = search(&db, "b1", "曹操", &model, "mock/kw", 2)
            .await
            .unwrap();
        assert_eq!(hits[0], 1);
        assert!(
            search(&db, "b2", "赤壁", &model, "mock/kw", 3)
                .await
                .unwrap()
                .iter()
                .all(|&i| i == 0)
        );
        // Rebuilding a book replaces its rows; deleting removes them.
        build(&db, "b1", &book, &model, "mock/kw", &|_, _| {})
            .await
            .unwrap();
        delete_book_passages(&db.conn(), "b2").unwrap();
        assert_eq!(
            status(&db, "b2", Some("mock/kw")).unwrap(),
            Status::default()
        );
        assert!(
            search(&db, "b2", "曹操", &model, "mock/kw", 3)
                .await
                .unwrap()
                .is_empty()
        );
        // Another model makes existing indexes stale.
        assert!(status(&db, "b1", Some("mock/other")).unwrap().stale);
        build(&db, "b2", &other, &model, "mock/other", &|_, _| {})
            .await
            .unwrap();
        assert_eq!(
            status(&db, "b1", Some("mock/kw")).unwrap(),
            Status::default(),
            "the old model's vectors are gone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
