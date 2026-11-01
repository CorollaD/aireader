//! Papers: telling a DOI or arXiv id from what the user typed or from a PDF, looking the
//! paper's details up, and reading the reference list at the end of a PDF.
//!
//! Everything that goes over the network starts with a request from the user, and sends only an
//! identifier, a title or one reference: never the text of a book.

use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::PageSource;

const USER_AGENT: &str = concat!(
    "AiReader/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/CorollaD/aireader)"
);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const STALL_TIMEOUT: Duration = Duration::from_secs(60);
const OPENALEX_FIELDS: &str = "title,publication_year,doi,type,authorships,primary_location,best_oa_location,biblio,abstract_inverted_index";
/// Longer than any paper people download by hand.
const MAX_PDF_BYTES: u64 = 300 << 20;
/// How far back from the end of a book the reference list is looked for.
const MAX_REFERENCE_PAGES: usize = 30;
const MIN_REFERENCES: usize = 3;
/// Keeps what follows the last entry (an appendix, say) from becoming part of it.
const MAX_REFERENCE_CHARS: usize = 700;
/// Pages read for an identifier: it is on the first page, or in a footnote soon after.
const ID_PAGES: usize = 2;
/// A title must be this long to identify a reference.
const MIN_TITLE_CHARS: usize = 12;

/// A paper's or book's bibliographic details.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PaperInfo {
    /// "article", "conference", "preprint", "book" or "other".
    pub kind: String,
    pub title: String,
    /// "Family, Given" when the two are known apart.
    pub authors: Vec<String>,
    pub year: Option<i32>,
    /// The journal, conference or book series.
    pub venue: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub doi: Option<String>,
    pub arxiv: Option<String>,
    pub url: Option<String>,
    pub summary: Option<String>,
    /// An open copy of the file, if one is known.
    pub pdf_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaperId {
    Doi(String),
    Arxiv(String),
}

impl std::fmt::Display for PaperId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaperId::Doi(doi) => write!(f, "{doi}"),
            PaperId::Arxiv(id) => write!(f, "arXiv:{id}"),
        }
    }
}

static DOI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b10\.\d{4,9}/[^\s"<>]+"#).unwrap());
static ARXIV_BARE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:arxiv:\s*)?(\d{4}\.\d{4,5}(?:v\d+)?|[a-z-]+(?:\.[a-z]{2})?/\d{7}(?:v\d+)?)$",
    )
    .unwrap()
});
static ARXIV_MARKED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:arxiv:\s*|arxiv\.org/(?:abs|pdf)/)(\d{4}\.\d{4,5}(?:v\d+)?|[a-z-]+(?:\.[a-z]{2})?/\d{7}(?:v\d+)?)")
        .unwrap()
});
static HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(?:\d+\.?\s+|[ivx]+\.?\s+)?(?:references|bibliography|works cited|literature cited|参考文献|參考文獻)\s*$")
        .unwrap()
});
static DOTTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(\d{1,3})\.\s+\S").unwrap());
static BROKEN_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\p{L})-\s*\n\s*(\p{Ll})").unwrap());
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());

fn clean_doi(doi: &str) -> String {
    doi.trim_end_matches(['.', ',', ';', ')', ']', '}'])
        .to_owned()
}

fn doi_id(doi: String) -> PaperId {
    // arXiv's own DOIs: its record is the better one.
    match doi.to_lowercase().strip_prefix("10.48550/arxiv.") {
        Some(id) => PaperId::Arxiv(id.to_owned()),
        None => PaperId::Doi(doi),
    }
}

fn marked_id(text: &str) -> Option<PaperId> {
    if let Some(m) = ARXIV_MARKED.captures(text) {
        return Some(PaperId::Arxiv(m[1].trim_end_matches(".pdf").to_owned()));
    }
    DOI.find(text).map(|m| doi_id(clean_doi(m.as_str())))
}

/// The identifier in what the user typed: a DOI, an arXiv id, or a link to either.
pub fn parse_id(input: &str) -> Option<PaperId> {
    let input = input.trim();
    if let Some(m) = ARXIV_BARE.captures(input) {
        return Some(PaperId::Arxiv(m[1].to_owned()));
    }
    marked_id(input)
}

/// The identifier printed in a paper, found without leaving this device.
pub fn detect_id(src: &dyn PageSource) -> Option<PaperId> {
    (0..src.page_count().min(ID_PAGES))
        .filter_map(|p| src.page_text(p))
        .find_map(|text| marked_id(&text))
}

/// Whitespace as a reader expects it, whatever the source's line breaks.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits a name into family and given names. Chinese, Japanese and Korean names stay whole.
pub fn name_parts(name: &str) -> (String, String) {
    let name = name.trim();
    if let Some((family, given)) = name.split_once(',') {
        return (family.trim().to_owned(), given.trim().to_owned());
    }
    match name.rsplit_once(char::is_whitespace) {
        Some((given, family)) if name.is_ascii() || !is_cjk(name) => {
            (family.trim().to_owned(), given.trim().to_owned())
        }
        _ => (name.to_owned(), String::new()),
    }
}

fn is_cjk(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}'))
}

/// A name as people write it.
pub fn display_name(name: &str) -> String {
    match name_parts(name) {
        (family, given) if given.is_empty() => family,
        (family, given) => format!("{given} {family}"),
    }
}

fn parse_arxiv(xml: &str, id: &str) -> Result<PaperInfo> {
    let doc = roxmltree::Document::parse(xml)?;
    let local = |n: &roxmltree::Node, name: &str| n.is_element() && n.tag_name().name() == name;
    let entry = doc
        .descendants()
        .find(|n| local(n, "entry"))
        .ok_or_else(|| anyhow!(crate::t!("paper_not_found")))?;
    let text = |name: &str| {
        entry
            .children()
            .find(|n| local(n, name))
            .and_then(|n| n.text())
            .map(squeeze)
            .filter(|t| !t.is_empty())
    };
    let title = text("title").ok_or_else(|| anyhow!(crate::t!("paper_not_found")))?;
    if text("id").is_some_and(|t| t.contains("/api/errors")) {
        bail!(crate::t!("paper_not_found"));
    }
    let authors = entry
        .children()
        .filter(|n| local(n, "author"))
        .filter_map(|a| a.children().find(|n| local(n, "name")))
        .filter_map(|n| n.text())
        .map(|name| {
            let (family, given) = name_parts(&squeeze(name));
            if given.is_empty() {
                family
            } else {
                format!("{family}, {given}")
            }
        })
        .collect();
    let bare = id.split('v').next().unwrap_or(id);
    Ok(PaperInfo {
        kind: "preprint".into(),
        title,
        authors,
        year: text("published").and_then(|d| d.get(..4)?.parse().ok()),
        venue: text("journal_ref"),
        doi: text("doi"),
        arxiv: Some(bare.to_owned()),
        url: Some(format!("https://arxiv.org/abs/{bare}")),
        summary: text("summary"),
        pdf_url: Some(format!("https://arxiv.org/pdf/{id}")),
        ..PaperInfo::default()
    })
}

fn kind_of(crossref_type: &str) -> &'static str {
    match crossref_type {
        "journal-article" | "article" | "review" => "article",
        "proceedings-article" => "conference",
        "posted-content" | "preprint" => "preprint",
        "book" | "monograph" | "edited-book" | "reference-book" | "book-chapter" => "book",
        _ => "other",
    }
}

fn parse_crossref(json: &str) -> Result<PaperInfo> {
    let root: Value = serde_json::from_str(json)?;
    crossref_item(&root["message"]).ok_or_else(|| anyhow!(crate::t!("paper_not_found")))
}

fn crossref_item(m: &Value) -> Option<PaperInfo> {
    let first = |name: &str| m[name][0].as_str().map(squeeze).filter(|t| !t.is_empty());
    let text = |name: &str| m[name].as_str().map(squeeze).filter(|t| !t.is_empty());
    let title = first("title")?;
    let authors = m["author"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| match (a["family"].as_str(), a["given"].as_str()) {
                    (Some(family), Some(given)) => Some(format!("{family}, {given}")),
                    (Some(family), None) => Some(family.to_owned()),
                    _ => a["name"].as_str().map(str::to_owned),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(PaperInfo {
        kind: kind_of(m["type"].as_str().unwrap_or("")).into(),
        title,
        authors,
        year: m["issued"]["date-parts"][0][0].as_i64().map(|y| y as i32),
        venue: first("container-title"),
        volume: text("volume"),
        issue: text("issue"),
        pages: text("page"),
        publisher: text("publisher"),
        doi: text("DOI"),
        url: text("URL"),
        summary: text("abstract").map(|a| squeeze(&TAGS.replace_all(&a, " "))),
        ..PaperInfo::default()
    })
}

/// OpenAlex gives abstracts as a map from each word to its positions.
fn inverted_abstract(index: &Value) -> Option<String> {
    let mut words: Vec<(u64, &str)> = index
        .as_object()?
        .iter()
        .flat_map(|(word, places)| {
            places
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .map(move |p| (p, word.as_str()))
        })
        .collect();
    words.sort();
    let text = words.iter().map(|(_, w)| *w).collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

fn parse_openalex(w: &Value) -> Option<PaperInfo> {
    let title = w["title"].as_str().map(squeeze).filter(|t| !t.is_empty())?;
    let authors = w["authorships"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| a["author"]["display_name"].as_str())
                .map(|name| match name_parts(name) {
                    (family, given) if given.is_empty() => family,
                    (family, given) => format!("{family}, {given}"),
                })
                .collect()
        })
        .unwrap_or_default();
    let biblio = |name: &str| w["biblio"][name].as_str().map(str::to_owned);
    let doi = w["doi"]
        .as_str()
        .map(|d| d.trim_start_matches("https://doi.org/").to_owned());
    let pages = match (biblio("first_page"), biblio("last_page")) {
        (Some(a), Some(b)) => Some(format!("{a}-{b}")),
        (a, _) => a,
    };
    Some(PaperInfo {
        kind: kind_of(w["type"].as_str().unwrap_or("")).into(),
        title,
        authors,
        year: w["publication_year"].as_i64().map(|y| y as i32),
        venue: w["primary_location"]["source"]["display_name"]
            .as_str()
            .map(str::to_owned),
        volume: biblio("volume"),
        issue: biblio("issue"),
        pages,
        url: doi.as_ref().map(|d| format!("https://doi.org/{d}")),
        doi,
        summary: inverted_abstract(&w["abstract_inverted_index"]),
        pdf_url: w["best_oa_location"]["pdf_url"].as_str().map(str::to_owned),
        ..PaperInfo::default()
    })
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(STALL_TIMEOUT)
        .build()?)
}

/// The text at `url`; `None` when there is no such record.
async fn get(url: &str) -> Result<Option<String>> {
    let network = |e: reqwest::Error| {
        log::warn!("paper lookup failed: {e}");
        anyhow!(crate::t!("paper_network"))
    };
    let response = client()?.get(url).send().await.map_err(network)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        log::warn!("paper lookup: {} from {url}", response.status());
        bail!(crate::t!("paper_network"));
    }
    Ok(Some(response.text().await.map_err(network)?))
}

/// A DOI as part of a URL path, where its slashes may stay.
fn doi_path(doi: &str) -> String {
    doi.replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23")
}

async fn openalex_work(doi: &str) -> Option<PaperInfo> {
    let path = doi_path(doi);
    let url = format!("https://api.openalex.org/works/doi:{path}?select={OPENALEX_FIELDS}");
    let body = get(&url).await.ok()??;
    parse_openalex(&serde_json::from_str(&body).ok()?)
}

/// Fills what the main record lacks from another source's.
fn fill(info: &mut PaperInfo, other: PaperInfo) {
    info.summary = info.summary.take().or(other.summary);
    info.pdf_url = info.pdf_url.take().or(other.pdf_url);
    info.pages = info.pages.take().or(other.pages);
    info.venue = info.venue.take().or(other.venue);
}

async fn lookup_doi(doi: &str) -> Result<PaperInfo> {
    let path = doi_path(doi);
    let crossref = get(&format!("https://api.crossref.org/works/{path}")).await?;
    let extra = openalex_work(doi).await;
    match (crossref, extra) {
        (Some(body), extra) => {
            let mut info = parse_crossref(&body)?;
            if let Some(extra) = extra {
                fill(&mut info, extra);
            }
            Ok(info)
        }
        (None, Some(info)) => Ok(info),
        (None, None) => bail!(crate::t!("paper_not_found")),
    }
}

async fn lookup_arxiv(id: &str) -> Result<PaperInfo> {
    let url = format!("https://export.arxiv.org/api/query?id_list={id}");
    let body = get(&url)
        .await?
        .ok_or_else(|| anyhow!(crate::t!("paper_not_found")))?;
    parse_arxiv(&body, id)
}

pub async fn lookup_id(id: &PaperId) -> Result<PaperInfo> {
    match id {
        PaperId::Doi(doi) => lookup_doi(doi).await,
        PaperId::Arxiv(id) => lookup_arxiv(id).await,
    }
}

/// Works matching a title or a whole reference. Crossref needs no key for this; OpenAlex covers
/// more preprints, but rations searches without one.
async fn search_crossref(text: &str) -> Result<Vec<PaperInfo>> {
    let mut url = reqwest::Url::parse("https://api.crossref.org/works")?;
    url.query_pairs_mut()
        .append_pair("query.bibliographic", text)
        .append_pair("rows", "5");
    let body = get(url.as_str()).await?.unwrap_or_default();
    let root: Value = serde_json::from_str(&body)?;
    Ok(root["message"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(crossref_item)
        .collect())
}

async fn search_openalex(text: &str) -> Vec<PaperInfo> {
    let Ok(mut url) = reqwest::Url::parse("https://api.openalex.org/works") else {
        return Vec::new();
    };
    url.query_pairs_mut()
        .append_pair("search", text)
        .append_pair("per-page", "5")
        .append_pair("select", OPENALEX_FIELDS);
    let Ok(Some(body)) = get(url.as_str()).await else {
        return Vec::new();
    };
    let root: Value = serde_json::from_str(&body).unwrap_or_default();
    root["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_openalex)
        .collect()
}

/// Adds what OpenAlex knows to a record found elsewhere: an abstract, a free copy.
async fn enriched(mut info: PaperInfo) -> PaperInfo {
    if let Some(doi) = info.doi.clone()
        && let Some(extra) = openalex_work(&doi).await
    {
        fill(&mut info, extra);
    }
    info
}

/// The work `text` names, going by its title; the first result of a title search otherwise, for
/// the user to judge, when `exact` isn't asked for.
async fn search(text: &str, exact: bool) -> Result<Option<PaperInfo>> {
    let found = search_crossref(text).await?;
    let hit = found.iter().find(|p| matches_reference(&p.title, text));
    let hit = match hit {
        Some(hit) => Some(hit.clone()),
        None => search_openalex(text)
            .await
            .into_iter()
            .find(|p| matches_reference(&p.title, text)),
    };
    let hit = if exact {
        hit
    } else {
        hit.or_else(|| found.into_iter().next())
    };
    Ok(match hit {
        Some(hit) => Some(enriched(hit).await),
        None => None,
    })
}

/// Looks up what the user typed: an identifier, or else a title.
pub async fn lookup(query: &str) -> Result<PaperInfo> {
    if let Some(id) = parse_id(query) {
        return lookup_id(&id).await;
    }
    search(&squeeze(query), false)
        .await?
        .ok_or_else(|| anyhow!(crate::t!("paper_not_found")))
}

/// Letters and digits only, for comparing titles.
fn folded(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether a record is the work a reference cites, going by its title. A search always returns
/// something, and showing the wrong paper is worse than showing none.
fn matches_reference(title: &str, reference: &str) -> bool {
    let title = folded(title);
    title.chars().count() >= MIN_TITLE_CHARS && folded(reference).contains(&title)
}

/// Finds the work a reference cites.
pub async fn lookup_reference(text: &str) -> Result<Option<PaperInfo>> {
    if let Some(id) = marked_id(text) {
        return lookup_id(&id).await.map(Some);
    }
    search(&squeeze(text), true).await
}

/// Downloads a paper's PDF to `dest`. Only https, and only what really is a PDF: many "open
/// access" links lead to a web page.
pub async fn download_pdf(url: &str, dest: &Path) -> Result<()> {
    use std::io::Write;
    if !url.starts_with("https://") {
        bail!(crate::t!("paper_no_open_copy"));
    }
    let failed = |e: reqwest::Error| {
        log::warn!("paper download failed: {e}");
        anyhow!(crate::t!("paper_network"))
    };
    let mut response = client()?
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(failed)?;
    let mut out = std::fs::File::create(dest)?;
    let mut written = 0u64;
    let mut checked = false;
    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        if !checked {
            if !chunk.starts_with(b"%PDF-") {
                bail!(crate::t!("paper_no_open_copy"));
            }
            checked = true;
        }
        written += chunk.len() as u64;
        if written > MAX_PDF_BYTES {
            bail!(crate::t!("paper_no_open_copy"));
        }
        out.write_all(&chunk)?;
    }
    if !checked {
        bail!(crate::t!("paper_no_open_copy"));
    }
    Ok(())
}

/// An entry of a paper's reference list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Reference {
    pub number: usize,
    pub text: String,
    pub page: usize,
}

fn entry_text(text: &str) -> String {
    let joined = BROKEN_WORD.replace_all(text, "$1$2");
    squeeze(&joined).chars().take(MAX_REFERENCE_CHARS).collect()
}

/// Entries numbered `[1]`, in order: each number is looked for after the one before, so a year
/// or a bracket in an entry's text can't start a new one.
fn bracketed(text: &str) -> Vec<(usize, usize, usize)> {
    let mut starts = Vec::new();
    let mut at = 0;
    for number in 1.. {
        let mark = format!("[{number}]");
        let Some(found) = text[at..].find(&mark) else {
            break;
        };
        starts.push((number, at + found, at + found + mark.len()));
        at += found + mark.len();
    }
    starts
}

/// Entries numbered `1.` at the start of a line, in order.
fn dotted(text: &str) -> Vec<(usize, usize, usize)> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some(m) = DOTTED.captures(line)
            && m[1].parse() == Ok(starts.len() + 1)
        {
            let number = starts.len() + 1;
            let end = offset + line.find('.').unwrap_or(0) + 1;
            starts.push((number, offset, end));
        }
        offset += line.len();
    }
    starts
}

/// Reads the numbered reference list of pages given as (page, text). Papers listing references
/// by author and year have no numbers to link to, and give nothing here.
fn parse_list(pages: &[(usize, String)]) -> Vec<Reference> {
    // The last heading of the kind: a table of contents may name it too.
    let mut heading = None;
    for (i, (_, text)) in pages.iter().enumerate() {
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            if HEADING.is_match(line.trim_end()) {
                heading = Some((i, offset + line.len()));
            }
            offset += line.len();
        }
    }
    let Some((first, skip)) = heading else {
        return Vec::new();
    };
    let mut text = String::new();
    let mut page_starts = Vec::new();
    for (i, (page, page_text)) in pages.iter().enumerate().skip(first) {
        page_starts.push((text.len(), *page));
        text.push_str(if i == first {
            &page_text[skip..]
        } else {
            page_text
        });
        text.push('\n');
    }
    let page_at = |offset: usize| {
        page_starts
            .iter()
            .take_while(|(start, _)| *start <= offset)
            .last()
            .map_or(0, |(_, page)| *page)
    };
    let mut starts = bracketed(&text);
    if starts.len() < MIN_REFERENCES {
        starts = dotted(&text);
    }
    if starts.len() < MIN_REFERENCES {
        return Vec::new();
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &(number, start, body))| {
            let end = starts.get(i + 1).map_or(text.len(), |next| next.1);
            Reference {
                number,
                text: entry_text(&text[body..end]),
                page: page_at(start),
            }
        })
        .collect()
}

pub fn references(src: &dyn PageSource) -> Vec<Reference> {
    let count = src.page_count();
    let pages: Vec<(usize, String)> = (count.saturating_sub(MAX_REFERENCE_PAGES)..count)
        .filter_map(|p| Some((p, src.page_text(p)?)))
        .collect();
    parse_list(&pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_in_every_form_people_paste() {
        let arxiv = |id: &str| Some(PaperId::Arxiv(id.into()));
        let doi = |id: &str| Some(PaperId::Doi(id.into()));
        assert_eq!(parse_id("1706.03762"), arxiv("1706.03762"));
        assert_eq!(parse_id(" arXiv:1706.03762v7 "), arxiv("1706.03762v7"));
        assert_eq!(
            parse_id("https://arxiv.org/abs/1706.03762"),
            arxiv("1706.03762")
        );
        assert_eq!(
            parse_id("https://arxiv.org/pdf/2301.12345v2.pdf"),
            arxiv("2301.12345v2")
        );
        assert_eq!(parse_id("hep-th/9901001"), arxiv("hep-th/9901001"));
        assert_eq!(parse_id("10.1038/nature14539"), doi("10.1038/nature14539"));
        assert_eq!(
            parse_id("https://doi.org/10.1038/nature14539."),
            doi("10.1038/nature14539")
        );
        assert_eq!(parse_id("doi:10.1145/3065386"), doi("10.1145/3065386"));
        assert_eq!(parse_id("10.48550/arXiv.1706.03762"), arxiv("1706.03762"));
        assert_eq!(parse_id("Attention Is All You Need"), None);
        assert_eq!(parse_id("2023.10"), None);
    }

    #[test]
    fn names_split_as_they_should() {
        assert_eq!(name_parts("LeCun, Yann"), ("LeCun".into(), "Yann".into()));
        assert_eq!(
            name_parts("Ashish Vaswani"),
            ("Vaswani".into(), "Ashish".into())
        );
        assert_eq!(name_parts("张三"), ("张三".into(), String::new()));
        assert_eq!(name_parts("Plato"), ("Plato".into(), String::new()));
        assert_eq!(display_name("LeCun, Yann"), "Yann LeCun");
        assert_eq!(display_name("张三"), "张三");
    }

    #[test]
    fn reads_an_arxiv_record() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
          <entry><id>http://arxiv.org/abs/1706.03762v7</id>
            <published>2017-06-12T17:57:34Z</published>
            <title>Attention Is All
              You Need</title>
            <summary> The dominant models.  </summary>
            <author><name>Ashish Vaswani</name></author>
            <author><name>Noam Shazeer</name></author>
            <arxiv:journal_ref>NeurIPS 2017</arxiv:journal_ref>
          </entry></feed>"#;
        let info = parse_arxiv(xml, "1706.03762v7").unwrap();
        assert_eq!(info.title, "Attention Is All You Need");
        assert_eq!(info.authors, ["Vaswani, Ashish", "Shazeer, Noam"]);
        assert_eq!(info.year, Some(2017));
        assert_eq!(info.venue.as_deref(), Some("NeurIPS 2017"));
        assert_eq!(info.arxiv.as_deref(), Some("1706.03762"));
        assert_eq!(
            info.pdf_url.as_deref(),
            Some("https://arxiv.org/pdf/1706.03762v7")
        );
    }

    #[test]
    fn an_unknown_arxiv_id_is_not_a_paper() {
        let empty = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>x</title></feed>"#;
        assert!(parse_arxiv(empty, "9999.99999").is_err());
    }

    #[test]
    fn reads_a_crossref_record() {
        let json = r#"{"message":{"title":["Deep learning"],"type":"journal-article","DOI":"10.1038/nature14539",
          "author":[{"given":"Yann","family":"LeCun"},{"name":"The Consortium"}],
          "container-title":["Nature"],"issued":{"date-parts":[[2015,5,27]]},"volume":"521","page":"436-444",
          "publisher":"Springer","abstract":"<jats:p>Deep  learning.</jats:p>"}}"#;
        let info = parse_crossref(json).unwrap();
        assert_eq!(info.kind, "article");
        assert_eq!(info.authors, ["LeCun, Yann", "The Consortium"]);
        assert_eq!(info.year, Some(2015));
        assert_eq!(info.venue.as_deref(), Some("Nature"));
        assert_eq!(info.summary.as_deref(), Some("Deep learning."));
    }

    #[test]
    fn reads_an_openalex_record_and_its_abstract() {
        let work = serde_json::json!({
            "title": "Deep learning", "publication_year": 2015, "doi": "https://doi.org/10.1/x", "type": "article",
            "authorships": [{"author": {"display_name": "Yann LeCun"}}],
            "primary_location": {"source": {"display_name": "Nature"}},
            "best_oa_location": {"pdf_url": "https://example.org/a.pdf"},
            "biblio": {"volume": "521", "first_page": "436", "last_page": "444"},
            "abstract_inverted_index": {"learning": [1, 3], "Deep": [0], "is": [2]},
        });
        let info = parse_openalex(&work).unwrap();
        assert_eq!(info.authors, ["LeCun, Yann"]);
        assert_eq!(info.doi.as_deref(), Some("10.1/x"));
        assert_eq!(info.pages.as_deref(), Some("436-444"));
        assert_eq!(info.summary.as_deref(), Some("Deep learning is learning"));
        assert_eq!(info.pdf_url.as_deref(), Some("https://example.org/a.pdf"));
    }

    fn page(page: usize, text: &str) -> (usize, String) {
        (page, text.to_owned())
    }

    #[test]
    fn reads_a_bracketed_reference_list() {
        let list = parse_list(&[
            page(
                7,
                "Conclusion\nWe cite [1] and [2].\nReferences\n[1] A. Author. A title. In-\nternational Journal, 2020.\n",
            ),
            page(
                8,
                "[2] B. Writer. Year [2019] in a title. 2021.\n[3] C. Scribe. Third. 2022.\n",
            ),
        ]);
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0].text,
            "A. Author. A title. International Journal, 2020."
        );
        assert_eq!(list[0].page, 7);
        assert_eq!(list[1].number, 2);
        assert!(list[1].text.contains("[2019]"), "{}", list[1].text);
        assert_eq!(list[2].page, 8);
    }

    #[test]
    fn reads_a_dotted_reference_list() {
        let list = parse_list(&[page(
            3,
            "参考文献\n1. 张三. 论文一. 2020.\n2. 李四. 论文二.\n续行 2021.\n3. 王五. 论文三. 2022.\n",
        )]);
        assert_eq!(list.len(), 3);
        assert_eq!(list[1].text, "李四. 论文二. 续行 2021.");
    }

    #[test]
    fn the_last_heading_counts() {
        let list = parse_list(&[
            page(0, "Contents\nReferences\n"),
            page(9, "References\n[1] One.\n[2] Two.\n[3] Three.\n"),
        ]);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].page, 9);
    }

    #[test]
    fn author_year_lists_and_plain_books_give_nothing() {
        assert!(
            parse_list(&[page(
                5,
                "References\nSmith, J. (2020). A title.\nLee, K. (2019). B.\n"
            )])
            .is_empty()
        );
        assert!(parse_list(&[page(5, "Chapter one\n[1] a note\n")]).is_empty());
    }

    #[test]
    fn reads_a_crossref_search() {
        let root: Value = serde_json::from_str(
            r#"{"message":{"items":[{"title":["Deep Residual Learning for Image Recognition"],"type":"proceedings-article"},{"title":[]}]}}"#,
        )
        .unwrap();
        let items: Vec<PaperInfo> = root["message"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(crossref_item)
            .collect();
        assert_eq!(items.len(), 1, "records without a title are skipped");
        assert_eq!(items[0].kind, "conference");
    }

    #[test]
    fn a_reference_only_matches_its_own_title() {
        let cited = "[7] K. He et al. Deep Residual Learning for Image Recognition. In CVPR, 2016.";
        assert!(matches_reference(
            "Deep residual learning for image recognition",
            cited
        ));
        assert!(
            !matches_reference("Deep learning", cited),
            "too short to tell"
        );
        assert!(!matches_reference(
            "Identity Mappings in Deep Residual Networks",
            cited
        ));
    }
}
