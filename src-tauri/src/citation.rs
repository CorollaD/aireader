//! Citations of books and papers: BibTeX, RIS and CSL-JSON for reference managers, and formatted
//! text in the usual styles (through hayagriva).

use std::collections::HashSet;

use anyhow::{Result, anyhow};
use hayagriva::archive::{ArchivedStyle, locales};
use hayagriva::citationberg::Style;
use hayagriva::{
    BibliographyDriver, BibliographyRequest, BufWriteFormat, CitationItem, CitationRequest,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::library::Book;
use crate::paper::{PaperInfo, name_parts};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CitationFormat {
    Bibtex,
    Ris,
    Csljson,
    Apa,
    Mla,
    Chicago,
    Ieee,
    Gbt7714,
}

impl CitationFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Bibtex => "bib",
            Self::Ris => "ris",
            Self::Csljson => "json",
            _ => "txt",
        }
    }

    fn style(self) -> Option<&'static str> {
        match self {
            Self::Apa => Some("apa"),
            Self::Mla => Some("modern-language-association"),
            Self::Chicago => Some("chicago-author-date"),
            Self::Ieee => Some("ieee"),
            Self::Gbt7714 => Some("gb-7714-2015-numeric"),
            _ => None,
        }
    }
}

/// What a citation needs: a paper's details, or what is known of a book.
fn details(book: &Book) -> PaperInfo {
    let mut info = book.paper.clone().unwrap_or_default();
    if info.kind.is_empty() {
        info.kind = "book".into();
    }
    if info.title.is_empty() {
        info.title = book.title.clone();
    }
    if info.authors.is_empty()
        && let Some(author) = &book.author
    {
        info.authors = split_authors(author);
    }
    info
}

/// Authors as typed in a book's details: "A, B", "A; B", "A and B", "A、B".
fn split_authors(text: &str) -> Vec<String> {
    text.replace(" and ", ";")
        .split([';', '；', '、', '，', ',', '&'])
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A citation key: first author's family name and the year, like `lecun2015`. Names outside
/// ASCII fall back on the book's id, which is at least stable.
fn key(book: &Book, info: &PaperInfo, taken: &mut HashSet<String>) -> String {
    let family = info
        .authors
        .first()
        .map(|a| name_parts(a).0)
        .unwrap_or_default();
    let family: String = family
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(|c| c.to_lowercase())
        .collect();
    let base = if family.is_empty() {
        format!("ref{}", &book.id[..book.id.len().min(8)])
    } else {
        format!(
            "{family}{}",
            info.year.map(|y| y.to_string()).unwrap_or_default()
        )
    };
    let mut key = base.clone();
    let mut suffix = 'a';
    while !taken.insert(key.clone()) {
        key = format!("{base}{suffix}");
        suffix = if suffix == 'z' {
            'a'
        } else {
            (suffix as u8 + 1) as char
        };
    }
    key
}

fn bibtex_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '\\' => "\\textbackslash{}".chars().collect::<Vec<_>>(),
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => vec!['\\', c],
            c => vec![c],
        })
        .collect()
}

fn bibtex_entry(key: &str, info: &PaperInfo) -> String {
    let kind = match info.kind.as_str() {
        "article" => "article",
        "conference" => "inproceedings",
        "book" => "book",
        _ => "misc",
    };
    let mut fields = vec![("title", format!("{{{}}}", bibtex_escape(&info.title)))];
    if !info.authors.is_empty() {
        let names: Vec<String> = info.authors.iter().map(|a| bibtex_escape(a)).collect();
        fields.push(("author", names.join(" and ")));
    }
    let venue_field = match info.kind.as_str() {
        "conference" => "booktitle",
        "article" => "journal",
        _ => "howpublished",
    };
    let mut add = |name, value: &Option<String>| {
        if let Some(value) = value {
            fields.push((name, bibtex_escape(value)));
        }
    };
    add(venue_field, &info.venue);
    add("volume", &info.volume);
    add("number", &info.issue);
    add("pages", &info.pages.as_ref().map(|p| p.replace('-', "--")));
    add("publisher", &info.publisher);
    add("year", &info.year.map(|y| y.to_string()));
    add("doi", &info.doi);
    add("url", &info.url);
    if let Some(id) = &info.arxiv {
        fields.push(("eprint", bibtex_escape(id)));
        fields.push(("archiveprefix", "arXiv".to_owned()));
    }
    let body: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("  {name} = {{{value}}}"))
        .collect();
    format!("@{kind}{{{key},\n{}\n}}\n", body.join(",\n"))
}

fn ris_entry(info: &PaperInfo) -> String {
    let kind = match info.kind.as_str() {
        "article" => "JOUR",
        "conference" => "CONF",
        "book" => "BOOK",
        "preprint" => "UNPB",
        _ => "GEN",
    };
    let mut lines = vec![format!("TY  - {kind}")];
    lines.extend(info.authors.iter().map(|a| format!("AU  - {a}")));
    lines.push(format!("TI  - {}", info.title));
    let mut add = |tag: &str, value: Option<&str>| {
        if let Some(value) = value {
            lines.push(format!("{tag}  - {value}"));
        }
    };
    let year = info.year.map(|y| y.to_string());
    let (start, end) = match info.pages.as_deref().map(|p| p.split_once('-')) {
        Some(Some((a, b))) => (Some(a), Some(b)),
        Some(None) => (info.pages.as_deref(), None),
        None => (None, None),
    };
    add("PY", year.as_deref());
    add("T2", info.venue.as_deref());
    add("VL", info.volume.as_deref());
    add("IS", info.issue.as_deref());
    add("SP", start);
    add("EP", end);
    add("PB", info.publisher.as_deref());
    add("DO", info.doi.as_deref());
    add("UR", info.url.as_deref());
    add("AB", info.summary.as_deref());
    lines.push("ER  - ".to_owned());
    lines.join("\n") + "\n"
}

fn csl_entry(key: &str, info: &PaperInfo) -> Value {
    let kind = match info.kind.as_str() {
        "article" => "article-journal",
        "conference" => "paper-conference",
        "book" => "book",
        "preprint" => "article",
        _ => "document",
    };
    let mut item = Map::new();
    item.insert("id".into(), json!(key));
    item.insert("type".into(), json!(kind));
    item.insert("title".into(), json!(info.title));
    let authors: Vec<Value> = info
        .authors
        .iter()
        .map(|a| match name_parts(a) {
            (family, given) if given.is_empty() => json!({"literal": family}),
            (family, given) => json!({"family": family, "given": given}),
        })
        .collect();
    if !authors.is_empty() {
        item.insert("author".into(), json!(authors));
    }
    if let Some(year) = info.year {
        item.insert("issued".into(), json!({"date-parts": [[year]]}));
    }
    let mut add = |name: &str, value: &Option<String>| {
        if let Some(value) = value {
            item.insert(name.into(), json!(value));
        }
    };
    add("container-title", &info.venue);
    add("volume", &info.volume);
    add("issue", &info.issue);
    add("page", &info.pages);
    add("publisher", &info.publisher);
    add("DOI", &info.doi);
    add("URL", &info.url);
    add("abstract", &info.summary);
    Value::Object(item)
}

/// The record hayagriva reads, in its own file format (JSON being YAML).
fn hayagriva_record(info: &PaperInfo) -> Value {
    let kind = match info.kind.as_str() {
        "book" => "Book",
        "other" => "Misc",
        "preprint" if info.venue.is_none() => "Misc",
        _ => "Article",
    };
    let mut item = Map::new();
    item.insert("type".into(), json!(kind));
    item.insert("title".into(), json!(info.title));
    if !info.authors.is_empty() {
        item.insert("author".into(), json!(info.authors));
    }
    if let Some(year) = info.year {
        item.insert("date".into(), json!(year.to_string()));
    }
    if let Some(publisher) = &info.publisher {
        item.insert("publisher".into(), json!(publisher));
    }
    if let Some(pages) = &info.pages {
        item.insert("page-range".into(), json!(pages));
    }
    let mut serial = Map::new();
    if let Some(doi) = &info.doi {
        serial.insert("doi".into(), json!(doi));
    }
    if let Some(id) = &info.arxiv {
        serial.insert("arxiv".into(), json!(id));
    }
    if !serial.is_empty() {
        item.insert("serial-number".into(), Value::Object(serial));
    }
    if let Some(url) = &info.url {
        item.insert("url".into(), json!(url));
    }
    if let Some(venue) = &info.venue {
        let mut parent = Map::new();
        let parent_kind = if info.kind == "conference" {
            "Proceedings"
        } else {
            "Periodical"
        };
        parent.insert("type".into(), json!(parent_kind));
        parent.insert("title".into(), json!(venue));
        if let Some(volume) = &info.volume {
            parent.insert("volume".into(), json!(volume));
        }
        if let Some(issue) = &info.issue {
            parent.insert("issue".into(), json!(issue));
        }
        item.insert("parent".into(), Value::Object(parent));
    }
    Value::Object(item)
}

fn styled(items: &[(String, PaperInfo)], name: &str) -> Result<String> {
    let library: Map<String, Value> = items
        .iter()
        .map(|(key, info)| (key.clone(), hayagriva_record(info)))
        .collect();
    let library = hayagriva::io::from_yaml_str(&serde_json::to_string(&library)?)
        .map_err(|e| anyhow!("citation data: {e}"))?;
    let style = ArchivedStyle::by_name(name)
        .ok_or_else(|| anyhow!("unknown style: {name}"))?
        .get();
    let Style::Independent(style) = style else {
        anyhow::bail!("unusable style: {name}");
    };
    let locales = locales();
    let mut driver = BibliographyDriver::new();
    for (key, _) in items {
        let entry = library
            .get(key)
            .ok_or_else(|| anyhow!("missing entry: {key}"))?;
        let cite = vec![CitationItem::with_entry(entry)];
        driver.citation(CitationRequest::from_items(cite, &style, &locales));
    }
    let rendered = driver.finish(BibliographyRequest {
        style: &style,
        locale: None,
        locale_files: &locales,
    });
    let bibliography = rendered
        .bibliography
        .ok_or_else(|| anyhow!("style has no bibliography: {name}"))?;
    let mut out = Vec::new();
    for item in bibliography.items {
        let mut line = String::new();
        if let Some(first) = &item.first_field {
            first.write_buf(&mut line, BufWriteFormat::Plain)?;
            line.push(' ');
        }
        item.content.write_buf(&mut line, BufWriteFormat::Plain)?;
        out.push(line);
    }
    Ok(out.join("\n") + "\n")
}

/// Citations of books in a format, in the order given.
pub fn cite(books: &[Book], format: CitationFormat) -> Result<String> {
    let mut taken = HashSet::new();
    let items: Vec<(String, PaperInfo)> = books
        .iter()
        .map(|book| {
            let info = details(book);
            (key(book, &info, &mut taken), info)
        })
        .collect();
    if let Some(style) = format.style() {
        return styled(&items, style);
    }
    Ok(match format {
        CitationFormat::Ris => items
            .iter()
            .map(|(_, i)| ris_entry(i))
            .collect::<Vec<_>>()
            .join("\n"),
        CitationFormat::Csljson => {
            let list: Vec<Value> = items.iter().map(|(k, i)| csl_entry(k, i)).collect();
            serde_json::to_string_pretty(&list)? + "\n"
        }
        _ => items
            .iter()
            .map(|(k, i)| bibtex_entry(k, i))
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: &str, title: &str, author: Option<&str>, paper: Option<PaperInfo>) -> Book {
        let mut b: Book = serde_json::from_value(json!({
            "id": id, "title": title, "author": author, "format": "PDF", "fileName": "a.pdf", "size": 1,
            "sha256": "0", "hasCover": false, "added": 0, "lastRead": null, "progress": 0.0, "position": null,
        }))
        .unwrap();
        b.paper = paper;
        b
    }

    fn deep_learning() -> PaperInfo {
        PaperInfo {
            kind: "article".into(),
            title: "Deep learning".into(),
            authors: vec![
                "LeCun, Yann".into(),
                "Bengio, Yoshua".into(),
                "Hinton, Geoffrey".into(),
            ],
            year: Some(2015),
            venue: Some("Nature".into()),
            volume: Some("521".into()),
            issue: Some("7553".into()),
            pages: Some("436-444".into()),
            doi: Some("10.1038/nature14539".into()),
            ..PaperInfo::default()
        }
    }

    fn paper() -> Book {
        book(
            "0123456789abcdef",
            "Deep learning",
            None,
            Some(deep_learning()),
        )
    }

    #[test]
    fn bibtex_is_ready_for_latex() {
        let text = cite(&[paper()], CitationFormat::Bibtex).unwrap();
        assert!(text.starts_with("@article{lecun2015,\n"), "{text}");
        assert!(text.contains("author = {LeCun, Yann and Bengio, Yoshua and Hinton, Geoffrey}"));
        assert!(text.contains("journal = {Nature}"));
        assert!(text.contains("pages = {436--444}"));
        let odd = book("00", "Costs & 100% of it_all", None, None);
        let text = cite(&[odd], CitationFormat::Bibtex).unwrap();
        assert!(text.contains(r"Costs \& 100\% of it\_all"), "{text}");
    }

    #[test]
    fn keys_stay_unique() {
        let text = cite(&[paper(), paper()], CitationFormat::Bibtex).unwrap();
        assert!(text.contains("@article{lecun2015,"));
        assert!(text.contains("@article{lecun2015a,"));
    }

    #[test]
    fn chinese_authors_get_a_usable_key() {
        let b = book("abcdef0123456789", "深度学习", Some("张三、李四"), None);
        let text = cite(&[b], CitationFormat::Bibtex).unwrap();
        assert!(text.starts_with("@book{refabcdef01,"), "{text}");
        assert!(text.contains("author = {张三 and 李四}"));
    }

    #[test]
    fn ris_has_its_tags_in_order() {
        let text = cite(&[paper()], CitationFormat::Ris).unwrap();
        assert!(
            text.starts_with("TY  - JOUR\nAU  - LeCun, Yann\n"),
            "{text}"
        );
        assert!(text.contains("SP  - 436\nEP  - 444\n"));
        assert!(text.ends_with("ER  - \n"));
    }

    #[test]
    fn csl_json_parses_back() {
        let text = cite(&[paper()], CitationFormat::Csljson).unwrap();
        let list: Vec<Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(list[0]["type"], "article-journal");
        assert_eq!(list[0]["author"][0]["family"], "LeCun");
        assert_eq!(list[0]["issued"]["date-parts"][0][0], 2015);
    }

    #[test]
    fn every_style_renders() {
        for format in [
            CitationFormat::Apa,
            CitationFormat::Mla,
            CitationFormat::Chicago,
            CitationFormat::Ieee,
            CitationFormat::Gbt7714,
        ] {
            let text = cite(&[paper()], format).unwrap();
            assert!(
                text.to_lowercase().contains("deep learning"),
                "{format:?}: {text}"
            );
            assert!(text.contains("2015"), "{format:?}: {text}");
            let plain = book("00", "没有更多信息的书", Some("张三"), None);
            let text = cite(&[plain], format).unwrap();
            assert!(text.contains("没有更多信息的书"), "{format:?}: {text}");
        }
    }

    #[test]
    fn the_styles_look_right() {
        let apa = cite(&[paper()], CitationFormat::Apa).unwrap();
        assert!(
            apa.starts_with("LeCun, Y., Bengio, Y., & Hinton, G. (2015)."),
            "{apa}"
        );
        assert!(apa.contains("Nature, 521(7553), 436–444"), "{apa}");
        let ieee = cite(&[paper()], CitationFormat::Ieee).unwrap();
        assert!(ieee.starts_with("[1] "), "{ieee}");
        let gbt = cite(&[paper()], CitationFormat::Gbt7714).unwrap();
        assert!(gbt.contains("[J/OL]"), "{gbt}");
    }

    #[test]
    fn a_book_without_details_is_still_citable() {
        let b = book("00", "A Title", Some("Jane Doe and John Roe"), None);
        let text = cite(&[b], CitationFormat::Bibtex).unwrap();
        assert!(text.starts_with("@book{doe,"), "{text}");
    }
}
