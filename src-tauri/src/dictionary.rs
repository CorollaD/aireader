//! Dictionaries the user adds (StarDict and MDict files), kept as they are under
//! `dictionaries/<id>/` in the library and read on demand.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use ego_tree::NodeRef;
use opendict::{DictEntry, Dictionary};
use scraper::{Html, Node};
use serde::{Deserialize, Serialize};

/// File types offered when adding a dictionary.
pub const EXTENSIONS: &[&str] = &["ifo", "mdx"];
/// MDict entries can be links to another entry: how many to follow.
const MAX_LINKS: usize = 3;
const LINK: &str = "@@@LINK=";

type Handle = Arc<Box<dyn Dictionary + Send + Sync>>;

/// An added dictionary, stored in the database's settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DictRecord {
    pub id: String,
    pub name: String,
    pub words: usize,
    pub enabled: bool,
}

/// What one dictionary says about a word.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Definition {
    pub dictionary: String,
    pub word: String,
    pub text: String,
}

/// The added dictionaries' files, opened on first use.
pub struct Dictionaries {
    root: PathBuf,
    open: Mutex<HashMap<String, Handle>>,
}

impl Dictionaries {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            open: Mutex::new(HashMap::new()),
        }
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn handle(&self, id: &str) -> Result<Handle> {
        if let Some(h) = self.open.lock().unwrap().get(id) {
            return Ok(h.clone());
        }
        let dir = self.dir(id);
        // The parsers read files from anywhere; a broken one must not bring the app down.
        let dict = std::panic::catch_unwind(|| opendict::open(&dir))
            .map_err(|_| anyhow!(crate::t!("dict_unsupported")))?
            .with_context(|| crate::t!("dict_unsupported"))?;
        let handle: Handle = Arc::new(dict);
        self.open
            .lock()
            .unwrap()
            .insert(id.to_owned(), handle.clone());
        Ok(handle)
    }

    /// Copies a dictionary into the library: `file` (`.ifo` or `.mdx`) and the files beside it
    /// that share its name (`.idx`, `.dict.dz`, `.mdd`…).
    pub fn import(&self, file: &Path) -> Result<DictRecord> {
        let stem = file
            .file_stem()
            .with_context(|| crate::t!("invalid_file_name"))?
            .to_string_lossy()
            .into_owned();
        let parent = file
            .parent()
            .with_context(|| crate::t!("invalid_file_name"))?;
        let id = format!("{:x}", crate::library::now_millis());
        let dir = self.dir(&id);
        std::fs::create_dir_all(&dir)?;
        let result = (|| {
            for entry in std::fs::read_dir(parent)?.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(&format!("{stem}.")) && entry.path().is_file() {
                    std::fs::copy(entry.path(), dir.join(&name))
                        .with_context(|| crate::t!("copy_failed"))?;
                }
            }
            let handle = self.handle(&id)?;
            let name = handle.info().name.trim();
            Ok(DictRecord {
                id: id.clone(),
                name: if name.is_empty() {
                    stem
                } else {
                    name.to_owned()
                },
                words: handle.word_count(),
                enabled: true,
            })
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        result
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        if id.is_empty() || id.contains(['/', '\\', '.']) {
            bail!("invalid dictionary id: {id}");
        }
        self.open.lock().unwrap().remove(id);
        std::fs::remove_dir_all(self.dir(id))?;
        Ok(())
    }

    /// Definitions of `word` from the enabled dictionaries, in their order.
    pub fn lookup(&self, records: &[DictRecord], word: &str) -> Vec<Definition> {
        let mut out = Vec::new();
        for record in records.iter().filter(|r| r.enabled) {
            let handle = match self.handle(&record.id) {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("dictionary {}: {e:#}", record.name);
                    continue;
                }
            };
            let found = candidates(word)
                .into_iter()
                .find_map(|w| lookup_one(handle.as_ref().as_ref(), &w).map(|e| (w, e)));
            if let Some((word, entries)) = found {
                let text = entries.iter().map(entry_text).collect::<Vec<_>>();
                out.push(Definition {
                    dictionary: record.name.clone(),
                    word,
                    text: text.join("\n\n"),
                });
            }
        }
        out
    }
}

/// The entries for exactly `word`, following links between entries.
fn lookup_one(dict: &(dyn Dictionary + Send + Sync), word: &str) -> Option<Vec<DictEntry>> {
    let mut word = word.to_owned();
    for _ in 0..=MAX_LINKS {
        let entries = dict
            .lookup(&word)
            .ok()
            .flatten()
            .or_else(|| dict.lookup_synonym(&word).ok().flatten())
            .filter(|e| !e.is_empty())?;
        let first = String::from_utf8_lossy(&entries[0].data);
        match first.trim().strip_prefix(LINK) {
            Some(target) => word = target.trim().to_owned(),
            None => return Some(entries),
        }
    }
    None
}

/// Forms to try for a selected word: as written, in other cases, and English endings removed.
pub fn candidates(word: &str) -> Vec<String> {
    let word = word.trim().trim_matches(|c: char| !c.is_alphanumeric());
    let lower = word.to_lowercase();
    let title: String = {
        let mut chars = lower.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    };
    let mut out = vec![word.to_owned(), lower.clone(), title];
    let mut stem = |ending: &str, replacement: &str| {
        if let Some(base) = lower.strip_suffix(ending).filter(|b| b.len() >= 2) {
            out.push(format!("{base}{replacement}"));
        }
    };
    stem("'s", "");
    stem("ies", "y");
    stem("ied", "y");
    stem("es", "");
    stem("s", "");
    stem("ing", "");
    stem("ing", "e");
    stem("ed", "");
    stem("ed", "e");
    stem("er", "");
    stem("est", "");
    // Doubled consonants: "running" → "run", "stopped" → "stop".
    for ending in ["ing", "ed"] {
        if let Some(base) = lower.strip_suffix(ending) {
            let mut chars = base.chars().rev();
            if let (Some(a), Some(b)) = (chars.next(), chars.next())
                && a == b
            {
                out.push(base[..base.len() - a.len_utf8()].to_owned());
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|w| !w.is_empty() && seen.insert(w.clone()));
    out
}

/// An entry as plain text. Entries may be plain or marked up (HTML, Pango, XDXF); markup is
/// reduced to text, so nothing from a dictionary file is ever rendered as a page.
fn entry_text(entry: &DictEntry) -> String {
    let data = String::from_utf8_lossy(&entry.data);
    match entry.type_id {
        'm' | 't' | 'l' => data.trim().to_owned(),
        _ => markup_to_text(&data),
    }
}

const BREAKS: &[&str] = &[
    "br",
    "p",
    "div",
    "li",
    "tr",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "blockquote",
    "ul",
    "ol",
    "table",
    "dd",
    "dt",
];
const HIDDEN: &[&str] = &["script", "style", "head", "link", "meta", "title"];

pub fn markup_to_text(markup: &str) -> String {
    fn walk(node: NodeRef<Node>, out: &mut String) {
        for child in node.children() {
            match child.value() {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) if HIDDEN.contains(&e.name()) => {}
                Node::Element(e) => {
                    let block = BREAKS.contains(&e.name());
                    if block {
                        out.push('\n');
                    }
                    walk(child, out);
                    if block {
                        out.push('\n');
                    }
                }
                _ => {}
            }
        }
    }
    let doc = Html::parse_fragment(markup);
    let mut out = String::new();
    walk(*doc.root_element(), &mut out);
    let lines: Vec<String> = out
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a tiny StarDict dictionary; `.ifo`, `.idx` (word, offset, size) and `.dict`.
    fn stardict(dir: &Path, entries: &[(&str, &str)], kind: &str) {
        let (mut idx, mut dict) = (Vec::new(), Vec::new());
        for (word, text) in entries {
            idx.extend_from_slice(word.as_bytes());
            idx.push(0);
            idx.extend_from_slice(&(dict.len() as u32).to_be_bytes());
            idx.extend_from_slice(&(text.len() as u32).to_be_bytes());
            dict.extend_from_slice(text.as_bytes());
        }
        std::fs::write(dir.join("demo.idx"), &idx).unwrap();
        std::fs::write(dir.join("demo.dict"), &dict).unwrap();
        std::fs::write(
            dir.join("demo.ifo"),
            format!(
                "StarDict's dict ifo file\nversion=2.4.2\nwordcount={}\nidxfilesize={}\nbookname=示例词典\nsametypesequence={kind}\n",
                entries.len(),
                idx.len()
            ),
        )
        .unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aireader-dict-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn imports_looks_up_and_removes_a_stardict_dictionary() {
        let dir = temp("star");
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        stardict(
            &src,
            &[
                ("apple", "<b>n.</b> 苹果<br/>a fruit<script>evil()</script>"),
                ("run", "v. 跑"),
            ],
            "h",
        );
        std::fs::write(src.join("other.txt"), "not part of it").unwrap();
        let dicts = Dictionaries::new(dir.join("lib"));
        let mut record = dicts.import(&src.join("demo.ifo")).unwrap();
        assert_eq!((record.name.as_str(), record.words), ("示例词典", 2));
        let stored = dir.join("lib").join(&record.id);
        assert!(stored.join("demo.dict").exists() && !stored.join("other.txt").exists());

        let records = vec![record.clone()];
        let found = dicts.lookup(&records, "Apples,");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].word, "apple");
        assert_eq!(found[0].text, "n. 苹果\na fruit");
        assert_eq!(dicts.lookup(&records, "running")[0].text, "v. 跑");
        assert!(dicts.lookup(&records, "zebra").is_empty());

        record.enabled = false;
        assert!(dicts.lookup(&[record.clone()], "run").is_empty());
        dicts.remove(&record.id).unwrap();
        assert!(!stored.exists());
        assert!(dicts.remove("../x").is_err());
    }

    #[test]
    fn imports_and_looks_up_an_mdict_dictionary() {
        let dir = temp("mdict");
        let src = dir.join("sample.mdx");
        std::fs::write(&src, include_bytes!("../tests/fixtures/test.mdx")).unwrap();
        let dicts = Dictionaries::new(dir.join("lib"));
        let record = dicts.import(&src).unwrap();
        assert_eq!(record.words, 3);
        let found = dicts.lookup(&[record], "Hello");
        assert_eq!(found[0].text, "hello greeting", "markup is removed");
    }

    #[test]
    fn a_file_that_is_no_dictionary_leaves_nothing() {
        let dir = temp("bad");
        std::fs::write(dir.join("x.mdx"), b"not a dictionary").unwrap();
        let dicts = Dictionaries::new(dir.join("lib"));
        assert!(dicts.import(&dir.join("x.mdx")).is_err());
        assert_eq!(std::fs::read_dir(dir.join("lib")).unwrap().count(), 0);
    }

    #[test]
    fn word_forms_to_try() {
        let forms = candidates("Running.");
        assert_eq!(forms[0], "Running");
        assert!(forms.contains(&"running".to_owned()) && forms.contains(&"run".to_owned()));
        assert!(candidates("stories").contains(&"story".to_owned()));
        assert!(candidates("made").contains(&"made".to_owned()));
        assert_eq!(candidates("孔明"), ["孔明"]);
        assert!(candidates("  ").is_empty());
    }

    #[test]
    fn markup_becomes_text_without_scripts() {
        assert_eq!(
            markup_to_text("<div>a <b>b</b></div><style>x{}</style><p>c</p><script>d</script>"),
            "a b\nc"
        );
    }
}
