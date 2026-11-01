//! Online catalogs: OPDS 1.x (Atom) and OPDS 2 (JSON) feeds, and the two sources with an API of
//! their own, Gutendex and Wikisource, all read into one shape. A feed lists entries that lead to
//! other feeds, or books with links to their files. This module only reads feeds; fetching is
//! done by the commands.

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const ATOM: &str = "http://www.w3.org/2005/Atom";
/// Long summaries are cut; the list only needs a taste.
const MAX_SUMMARY: usize = 280;
/// Covers embedded in a feed bigger than this are left out, to keep responses small.
const MAX_DATA_COVER: usize = 64 << 10;

/// Download relations that give the file itself: buying and borrowing don't.
const ACQUISITION: &[&str] = &[
    "http://opds-spec.org/acquisition",
    "http://opds-spec.org/acquisition/open-access",
];
/// What the Wikisource export service is asked for.
const WIKISOURCE_EXPORT: &str = "https://ws-export.wmcloud.org/";
const THUMBNAIL: &[&str] = &[
    "http://opds-spec.org/image/thumbnail",
    "http://opds-spec.org/thumbnail",
    "x-stanza-cover-image-thumbnail",
];
const IMAGE: &[&str] = &[
    "http://opds-spec.org/image",
    "http://opds-spec.org/cover",
    "x-stanza-cover-image",
];

/// A catalog the user added. Its password is in the system keychain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub username: String,
    /// "" for an OPDS catalog, or "gutendex" or "wikisource".
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Download {
    pub url: String,
    pub mime: String,
    /// "EPUB", "PDF"…
    pub format: String,
    /// The extension to save it with.
    pub extension: String,
    /// What the feed calls this file, e.g. "EPUB (no images)".
    pub title: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub summary: String,
    /// An address, or a `data:` URL when the feed embeds the picture.
    pub cover: Option<String>,
    /// The feed this entry leads to.
    pub link: Option<String>,
    pub downloads: Vec<Download>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Feed {
    pub title: String,
    pub entries: Vec<Entry>,
    /// The next page.
    pub next: Option<String>,
    /// How to search: a template with `{searchTerms}`, or a description to fetch.
    pub search: Option<String>,
}

/// The format of a media type the reader can open.
fn format_of(mime: &str) -> Option<(&'static str, &'static str)> {
    let mime = mime.split(';').next().unwrap_or("").trim().to_lowercase();
    Some(match mime.as_str() {
        "application/epub+zip" => ("EPUB", "epub"),
        "application/pdf" => ("PDF", "pdf"),
        "application/x-mobipocket-ebook" | "application/x-mobi8-ebook" => ("MOBI", "mobi"),
        "application/vnd.amazon.ebook" | "application/vnd.amazon.mobi8-ebook" => ("AZW3", "azw3"),
        "application/fb2" | "application/x-fictionbook+xml" | "text/fb2+xml" => ("FB2", "fb2"),
        "application/fb2+zip" | "application/x-zip-compressed-fb2" => ("FB2", "fb2.zip"),
        "application/vnd.comicbook+zip" | "application/x-cbz" => ("CBZ", "cbz"),
        "image/vnd.djvu" | "image/x-djvu" => ("DJVU", "djvu"),
        "application/rtf" | "text/rtf" => ("RTF", "rtf"),
        "application/vnd.oasis.opendocument.text" => ("ODT", "odt"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            ("DOCX", "docx")
        }
        "text/plain" => ("TXT", "txt"),
        "text/markdown" => ("MD", "md"),
        _ => return None,
    })
}

/// `href` as an absolute web address; anything else (`file:`, `javascript:`…) is dropped.
fn absolute(base: &Url, href: &str) -> Option<String> {
    let url = base.join(href.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

fn is_atom<'a>(node: &Node<'a, 'a>, name: &str) -> bool {
    node.is_element() && node.has_tag_name((ATOM, name))
}

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|n| is_atom(n, name))
}

/// The text inside an element, including nested (XHTML) elements, with spaces tidied.
fn text_of(node: Node) -> String {
    let parts: Vec<&str> = node
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    parts
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn summary_of(entry: Node) -> String {
    let text = ["summary", "content"]
        .iter()
        .find_map(|name| child(entry, name).map(text_of).filter(|t| !t.is_empty()))
        .unwrap_or_default();
    // An escaped HTML `content` arrives as text with tags in it.
    let text = if text.contains('<') {
        crate::dictionary::markup_to_text(&text).replace('\n', " ")
    } else {
        text
    };
    match text.char_indices().nth(MAX_SUMMARY) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

fn links<'a>(node: Node<'a, 'a>) -> impl Iterator<Item = Node<'a, 'a>> {
    node.children().filter(|n| is_atom(n, "link"))
}

fn cover_of(entry: Node, base: &Url) -> Option<String> {
    let find = |rels: &[&str]| {
        links(entry).find_map(|l| {
            let href = l.attribute("href")?;
            if !rels.contains(&l.attribute("rel")?) {
                return None;
            }
            if href.starts_with("data:image/") {
                return (href.len() <= MAX_DATA_COVER).then(|| href.to_owned());
            }
            absolute(base, href)
        })
    };
    find(THUMBNAIL).or_else(|| find(IMAGE))
}

fn read_entry(entry: Node, base: &Url) -> Entry {
    let downloads: Vec<Download> = links(entry)
        .filter_map(|l| {
            if !ACQUISITION.contains(&l.attribute("rel")?) {
                return None;
            }
            let mime = l.attribute("type")?;
            let (format, extension) = format_of(mime)?;
            Some(Download {
                url: absolute(base, l.attribute("href")?)?,
                mime: mime.to_owned(),
                format: format.to_owned(),
                extension: extension.to_owned(),
                title: l.attribute("title").map(str::to_owned),
            })
        })
        .collect();
    // Prefer a link marked as a subsection; plain Atom links are the next best thing.
    let atom = |l: &Node| l.attribute("type").is_some_and(|t| t.contains("atom+xml"));
    let link = links(entry)
        .find(|l| atom(l) && l.attribute("rel") == Some("subsection"))
        .or_else(|| {
            links(entry).find(|l| atom(l) && !matches!(l.attribute("rel"), Some("self" | "up")))
        })
        .and_then(|l| absolute(base, l.attribute("href")?));
    Entry {
        id: child(entry, "id").map(text_of).unwrap_or_default(),
        title: child(entry, "title").map(text_of).unwrap_or_default(),
        author: child(entry, "author")
            .and_then(|a| child(a, "name"))
            .map(text_of)
            .filter(|a| !a.is_empty()),
        summary: summary_of(entry),
        cover: cover_of(entry, base),
        link: if downloads.is_empty() { link } else { None },
        downloads,
    }
}

/// Reads a feed fetched from `url`.
pub fn parse_feed(xml: &str, url: &str) -> Result<Feed> {
    let base = Url::parse(url).context("invalid address")?;
    let doc = Document::parse(xml).map_err(|e| anyhow!("not an XML feed: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name((ATOM, "feed")) {
        bail!("not an Atom feed");
    }
    let by_rel = |rel: &str| {
        links(root)
            .find(|l| l.attribute("rel") == Some(rel))
            .and_then(|l| l.attribute("href"))
    };
    let search = links(root)
        .filter(|l| l.attribute("rel") == Some("search"))
        .filter_map(|l| {
            let href = l.attribute("href")?;
            // A template's braces aren't valid in a URL, so it isn't resolved as one.
            let template = href.contains("{searchTerms}");
            let url = absolute(&base, href)?;
            Some((template, url.replace("%7BsearchTerms%7D", "{searchTerms}")))
        })
        .max_by_key(|(template, _)| *template)
        .map(|(_, url)| url);
    Ok(Feed {
        title: child(root, "title").map(text_of).unwrap_or_default(),
        entries: root
            .children()
            .filter(|n| is_atom(n, "entry"))
            .map(|e| read_entry(e, &base))
            // Nothing to show or click: some catalogs have a few of these.
            .filter(|e| !e.title.is_empty())
            .collect(),
        next: by_rel("next").and_then(|h| absolute(&base, h)),
        search,
    })
}

fn cut_summary(text: &str) -> String {
    let text = if text.contains('<') {
        crate::dictionary::markup_to_text(text).replace('\n', " ")
    } else {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    match text.char_indices().nth(MAX_SUMMARY) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

/// A link's relations: one, or a list.
fn rels(link: &Value) -> Vec<&str> {
    match &link["rel"] {
        Value::String(rel) => vec![rel.as_str()],
        Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// A search template in the form OPDS 1 uses: `{?query}` becomes `?query={searchTerms}`.
fn opds2_search(link: &Value, base: &Url) -> Option<String> {
    let href = link["href"].as_str()?;
    let (head, rest) = href.split_once('{')?;
    let (names, tail) = rest.split_once('}')?;
    let marker = names.chars().next().filter(|c| matches!(c, '?' | '&'))?;
    let names = &names[1..];
    names
        .split(',')
        .any(|n| n.trim() == "query")
        .then_some(())?;
    let head = absolute(base, head)?;
    Some(format!("{head}{marker}query={{searchTerms}}{tail}"))
}

/// Names from a `string`, an `{ "name": … }` object, or a list of either.
fn contributor(value: &Value) -> Option<String> {
    match value {
        Value::String(name) => Some(name.clone()),
        Value::Object(_) => value["name"].as_str().map(str::to_owned),
        Value::Array(list) => {
            let names: Vec<String> = list.iter().filter_map(contributor).collect();
            (!names.is_empty()).then(|| names.join(", "))
        }
        _ => None,
    }
}

/// Titles and subjects can come as language maps.
fn plain_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => map.values().find_map(Value::as_str).map(str::to_owned),
        _ => None,
    }
}

fn opds2_publication(publication: &Value, base: &Url) -> Option<Entry> {
    let metadata = &publication["metadata"];
    let title = plain_text(&metadata["title"])?;
    let links = publication["links"].as_array()?;
    let downloads = links
        .iter()
        .filter(|l| rels(l).iter().any(|r| ACQUISITION.contains(r)))
        .filter_map(|l| {
            let mime = l["type"].as_str()?;
            let (format, extension) = format_of(mime)?;
            Some(Download {
                url: absolute(base, l["href"].as_str()?)?,
                mime: mime.to_owned(),
                format: format.to_owned(),
                extension: extension.to_owned(),
                title: l["title"].as_str().map(str::to_owned),
            })
        })
        .collect();
    let cover = publication["images"]
        .as_array()
        .and_then(|list| list.first())
        .and_then(|image| absolute(base, image["href"].as_str()?));
    Some(Entry {
        id: metadata["identifier"].as_str().unwrap_or(&title).to_owned(),
        author: contributor(&metadata["author"]),
        summary: metadata["description"]
            .as_str()
            .map(cut_summary)
            .unwrap_or_default(),
        cover,
        link: None,
        downloads,
        title,
    })
}

fn opds2_navigation(link: &Value, base: &Url) -> Option<Entry> {
    let title = link["title"].as_str()?.to_owned();
    let href = absolute(base, link["href"].as_str()?)?;
    Some(Entry {
        id: href.clone(),
        title,
        author: None,
        summary: String::new(),
        cover: None,
        link: Some(href),
        downloads: Vec::new(),
    })
}

fn opds2_entries(node: &Value, base: &Url, entries: &mut Vec<Entry>) {
    let list = |name: &str| node[name].as_array().cloned().unwrap_or_default();
    entries.extend(
        list("navigation")
            .iter()
            .filter_map(|l| opds2_navigation(l, base)),
    );
    entries.extend(
        list("publications")
            .iter()
            .filter_map(|p| opds2_publication(p, base)),
    );
}

/// Reads an OPDS 2 feed fetched from `url`. A group's books are listed under the feed itself;
/// a group too long to list links to its own feed, and then appears as one entry.
fn parse_opds2(json: &str, url: &str) -> Result<Feed> {
    let base = Url::parse(url).context("invalid address")?;
    let root: Value = serde_json::from_str(json).context("not a JSON feed")?;
    if !root.is_object() || root.get("metadata").is_none() {
        bail!("not an OPDS 2 feed");
    }
    let mut entries = Vec::new();
    opds2_entries(&root, &base, &mut entries);
    for group in root["groups"].as_array().into_iter().flatten() {
        let more = group["links"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| rels(l).contains(&"self"))
            .and_then(|l| absolute(&base, l["href"].as_str()?));
        match (group["metadata"]["title"].as_str(), more) {
            (Some(title), Some(href)) => entries.push(Entry {
                id: href.clone(),
                title: title.to_owned(),
                author: None,
                summary: String::new(),
                cover: None,
                link: Some(href),
                downloads: Vec::new(),
            }),
            _ => opds2_entries(group, &base, &mut entries),
        }
    }
    let links = root["links"].as_array().cloned().unwrap_or_default();
    let by_rel = |rel: &str| links.iter().find(|l| rels(l).contains(&rel));
    Ok(Feed {
        title: root["metadata"]["title"].as_str().unwrap_or("").to_owned(),
        entries,
        next: by_rel("next").and_then(|l| absolute(&base, l["href"].as_str()?)),
        search: by_rel("search").and_then(|l| opds2_search(l, &base)),
    })
}

/// Reads a feed of either OPDS version.
pub fn parse_any(body: &str, url: &str) -> Result<Feed> {
    if body.trim_start().starts_with('{') {
        parse_opds2(body, url)
    } else {
        parse_feed(body, url)
    }
}

fn gutendex_book(book: &Value) -> Option<Entry> {
    let title = book["title"].as_str()?.to_owned();
    let formats = book["formats"].as_object()?;
    let mut downloads: Vec<Download> = Vec::new();
    for (mime, link) in formats {
        let (Some(link), Some((format, extension))) = (link.as_str(), format_of(mime)) else {
            continue;
        };
        // Zipped copies aren't what their extension says, and one link per format is plenty.
        if link.ends_with(".zip") || downloads.iter().any(|d| d.format == format) {
            continue;
        }
        downloads.push(Download {
            url: link.to_owned(),
            mime: mime.clone(),
            format: format.to_owned(),
            extension: extension.to_owned(),
            title: None,
        });
    }
    downloads.sort_by_key(|d| d.format != "EPUB");
    Some(Entry {
        id: book["id"].to_string(),
        author: contributor(&book["authors"]),
        summary: book["summaries"][0]
            .as_str()
            .map(cut_summary)
            .unwrap_or_default(),
        cover: formats
            .get("image/jpeg")
            .and_then(Value::as_str)
            .map(str::to_owned),
        link: None,
        downloads,
        title,
    })
}

/// Reads a page of Gutendex, which lists Project Gutenberg with a filter by language.
fn parse_gutendex(json: &str, title: &str) -> Result<Feed> {
    let root: Value = serde_json::from_str(json).context("not a JSON page")?;
    let results = root["results"].as_array().context("no results")?;
    Ok(Feed {
        title: title.to_owned(),
        entries: results.iter().filter_map(gutendex_book).collect(),
        next: root["next"].as_str().map(str::to_owned),
        search: None,
    })
}

/// The search address of a Wikisource: its own search, for main-namespace pages.
fn wikisource_search(site: &str) -> String {
    format!(
        "{}/w/api.php?action=query&list=search&srnamespace=0&srlimit=30&format=json&formatversion=2&srsearch={{searchTerms}}",
        site.trim_end_matches('/')
    )
}

/// A Wikisource to search: it has no list worth browsing.
pub fn wikisource_home(catalog: &Catalog) -> Feed {
    Feed {
        title: catalog.title.clone(),
        entries: Vec::new(),
        next: None,
        search: Some(wikisource_search(&catalog.url)),
    }
}

fn wikisource_language(site: &Url) -> Option<&str> {
    site.host_str()?.strip_suffix(".wikisource.org")
}

fn wikisource_entry(page: &Value, lang: &str) -> Option<Entry> {
    let title = page["title"].as_str()?.to_owned();
    // A subpage is a chapter of a work that has its own result.
    if title.contains('/') {
        return None;
    }
    let mut url = Url::parse(WIKISOURCE_EXPORT).ok()?;
    url.query_pairs_mut()
        .append_pair("lang", lang)
        .append_pair("page", &title.replace(' ', "_"))
        .append_pair("format", "epub");
    Some(Entry {
        id: format!("{lang}:{title}"),
        author: None,
        summary: page["snippet"]
            .as_str()
            .map(cut_summary)
            .unwrap_or_default(),
        cover: None,
        link: None,
        downloads: vec![Download {
            url: url.to_string(),
            mime: "application/epub+zip".to_owned(),
            format: "EPUB".to_owned(),
            extension: "epub".to_owned(),
            title: None,
        }],
        title,
    })
}

/// Reads search results of a Wikisource, fetched from `url`.
fn parse_wikisource(json: &str, url: &str) -> Result<Feed> {
    let base = Url::parse(url).context("invalid address")?;
    let lang = wikisource_language(&base)
        .context("not a Wikisource")?
        .to_owned();
    let root: Value = serde_json::from_str(json).context("not a JSON page")?;
    let results = root["query"]["search"].as_array().context("no results")?;
    let next = root["continue"]["sroffset"].as_u64().map(|offset| {
        let kept: Vec<(String, String)> = base
            .query_pairs()
            .filter(|(k, _)| k != "sroffset")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let mut url = base.clone();
        url.query_pairs_mut()
            .clear()
            .extend_pairs(kept)
            .append_pair("sroffset", &offset.to_string());
        url.to_string()
    });
    Ok(Feed {
        title: String::new(),
        entries: results
            .iter()
            .filter_map(|p| wikisource_entry(p, &lang))
            .collect(),
        next,
        search: None,
    })
}

/// Reads a page of a catalog, by the catalog's kind.
pub fn parse_page(catalog: &Catalog, body: &str, url: &str) -> Result<Feed> {
    match catalog.kind.as_str() {
        "gutendex" => {
            let mut feed = parse_gutendex(body, &catalog.title)?;
            feed.search = Some(gutendex_search(&catalog.url));
            Ok(feed)
        }
        "wikisource" => {
            let mut feed = parse_wikisource(body, url)?;
            feed.search = Some(wikisource_search(&catalog.url));
            Ok(feed)
        }
        _ => parse_any(body, url),
    }
}

fn gutendex_search(catalog: &str) -> String {
    let separator = if catalog.contains('?') { '&' } else { '?' };
    format!("{catalog}{separator}search={{searchTerms}}")
}

/// The search template in an OpenSearch description fetched from `url`.
pub fn parse_search_description(xml: &str, url: &str) -> Result<String> {
    let base = Url::parse(url).context("invalid address")?;
    let doc = Document::parse(xml).map_err(|e| anyhow!("not an XML document: {e}"))?;
    let template = doc
        .descendants()
        .filter(|n| n.has_tag_name("Url") || n.tag_name().name() == "Url")
        .filter(|n| n.attribute("type").is_some_and(|t| t.contains("atom+xml")))
        .find_map(|n| n.attribute("template"))
        .context("no search address")?;
    absolute(&base, template)
        .map(|u| u.replace("%7BsearchTerms%7D", "{searchTerms}"))
        .context("no search address")
}

/// `template` with the words to look for filled in.
pub fn search_url(template: &str, query: &str) -> String {
    let mut url = Url::parse("http://x/").expect("valid URL");
    url.query_pairs_mut().append_pair("q", query.trim());
    let encoded = url
        .query()
        .unwrap_or("")
        .trim_start_matches("q=")
        .to_owned();
    template
        .replace("{searchTerms}", &encoded)
        .replace("{startPage?}", "1")
        .replace("{startIndex?}", "0")
        .replace("{count?}", "")
}

/// Where a file really is, when a server answers with its redirect written out as JSON, as the
/// Internet Archive's OPDS service does. Only https on the same site is followed.
pub fn wrapped_redirect(body: &[u8], url: &str) -> Option<String> {
    let reply: Value = serde_json::from_slice(body).ok()?;
    let status = reply["status_code"].as_u64()?;
    let target = reply["_headers"]["location"].as_str()?;
    ((300..400).contains(&status) && target.starts_with("https://") && same_host(url, target))
        .then(|| target.to_owned())
}

/// The `Authorization` value answering a server's Digest challenge.
pub fn digest_answer(
    challenge: &str,
    url: &str,
    username: &str,
    password: Option<&str>,
) -> Option<String> {
    if !challenge.trim_start().to_lowercase().starts_with("digest") {
        return None;
    }
    let target = Url::parse(url).ok()?;
    let path = match target.query() {
        Some(query) => format!("{}?{query}", target.path()),
        None => target.path().to_owned(),
    };
    let context = digest_auth::AuthContext::new(username, password.unwrap_or(""), &path);
    let answer = digest_auth::parse(challenge).ok()?.respond(&context).ok()?;
    Some(answer.to_header_string())
}

/// Whether credentials for `catalog` may be sent to `url`: only to the same host.
pub fn same_host(catalog: &str, url: &str) -> bool {
    match (Url::parse(catalog), Url::parse(url)) {
        (Ok(a), Ok(b)) => {
            a.host_str() == b.host_str() && a.port_or_known_default() == b.port_or_known_default()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPDS2: &str = r#"{
      "metadata": {"title": "Archive.org"},
      "links": [
        {"rel": "self", "href": "/opds", "type": "application/opds+json"},
        {"rel": "search", "href": "/opds/search{?query,language}", "templated": true},
        {"rel": ["next", "last"], "href": "/opds?page=2"}
      ],
      "navigation": [{"href": "/opds/ebooks", "title": "eBooks", "type": "application/opds+json"}],
      "publications": [
        {"metadata": {"title": "Open book", "identifier": "urn:1",
                      "author": [{"name": "A. Writer"}, "B. Writer"], "description": "<p>About it</p>"},
         "links": [
           {"rel": "http://opds-spec.org/acquisition/open-access", "href": "/f/a.epub", "type": "application/epub+zip"},
           {"rel": "http://opds-spec.org/acquisition/borrow", "href": "/f/b.epub", "type": "application/epub+zip"},
           {"rel": "self", "href": "/p/1", "type": "application/opds-publication+json"}],
         "images": [{"href": "/c/1.jpg", "type": "image/jpeg"}]},
        {"metadata": {"title": {"en": "Lent book"}},
         "links": [{"rel": "http://opds-spec.org/acquisition/borrow", "href": "/f/c.epub", "type": "application/epub+zip"}]}
      ],
      "groups": [
        {"metadata": {"title": "Trending"},
         "links": [{"rel": ["self", "home"], "href": "/opds/trending"}]},
        {"metadata": {"title": "Short"},
         "publications": [{"metadata": {"title": "In a group"}, "links": []}]}
      ]
    }"#;

    #[test]
    fn reads_an_opds2_feed() {
        let feed = parse_any(OPDS2, "https://archive.org/opds").unwrap();
        assert_eq!(feed.title, "Archive.org");
        assert_eq!(
            feed.next.as_deref(),
            Some("https://archive.org/opds?page=2")
        );
        let search = feed.search.as_deref().unwrap();
        assert_eq!(
            search,
            "https://archive.org/opds/search?query={searchTerms}"
        );
        assert_eq!(
            search_url(search, "深度 学习"),
            "https://archive.org/opds/search?query=%E6%B7%B1%E5%BA%A6+%E5%AD%A6%E4%B9%A0"
        );
        let find = |title: &str| feed.entries.iter().find(|e| e.title == title).unwrap();
        assert_eq!(
            find("eBooks").link.as_deref(),
            Some("https://archive.org/opds/ebooks")
        );

        let open = find("Open book");
        assert_eq!(open.author.as_deref(), Some("A. Writer, B. Writer"));
        assert_eq!(open.summary, "About it");
        assert_eq!(open.cover.as_deref(), Some("https://archive.org/c/1.jpg"));
        assert_eq!(open.downloads.len(), 1, "borrowing is not a download");
        assert_eq!(open.downloads[0].url, "https://archive.org/f/a.epub");

        assert!(find("Lent book").downloads.is_empty());
        assert_eq!(
            find("Trending").link.as_deref(),
            Some("https://archive.org/opds/trending")
        );
        assert!(feed.entries.iter().any(|e| e.title == "In a group"));
    }

    #[test]
    fn json_that_is_no_feed_is_refused() {
        assert!(parse_any("{\"error\": \"nope\"}", "https://a.org/").is_err());
        assert!(parse_any("{broken", "https://a.org/").is_err());
    }

    #[test]
    fn atom_still_goes_to_the_atom_reader() {
        assert_eq!(
            parse_any(LIST, "https://www.gutenberg.org/").unwrap().title,
            "Project Gutenberg"
        );
    }

    #[test]
    fn opens_the_new_formats() {
        assert_eq!(format_of("image/vnd.djvu"), Some(("DJVU", "djvu")));
        assert_eq!(format_of("application/vnd.comicbook-rar"), None);
        assert_eq!(format_of("text/rtf; charset=utf-8"), Some(("RTF", "rtf")));
        assert_eq!(
            format_of("application/vnd.oasis.opendocument.text"),
            Some(("ODT", "odt"))
        );
    }

    fn catalog(kind: &str, url: &str) -> Catalog {
        Catalog {
            id: "c".into(),
            title: "书源".into(),
            url: url.into(),
            username: String::new(),
            kind: kind.into(),
        }
    }

    #[test]
    fn reads_gutendex() {
        let body = r#"{"count": 2, "next": "https://gutendex.com/books?languages=zh&page=2", "results": [
          {"id": 23962, "title": "西遊記", "authors": [{"name": "Wu, Cheng'en"}], "summaries": ["A novel."],
           "formats": {"application/epub+zip": "https://www.gutenberg.org/ebooks/23962.epub3.images",
                       "text/plain; charset=utf-8": "https://www.gutenberg.org/ebooks/23962.txt.utf-8",
                       "text/plain; charset=us-ascii": "https://www.gutenberg.org/x.zip",
                       "application/octet-stream": "https://www.gutenberg.org/y",
                       "image/jpeg": "https://www.gutenberg.org/cover.jpg"}}]}"#;
        let source = catalog("gutendex", "https://gutendex.com/books?languages=zh");
        let feed = parse_page(&source, body, &source.url).unwrap();
        assert_eq!(
            feed.next.as_deref(),
            Some("https://gutendex.com/books?languages=zh&page=2")
        );
        assert_eq!(
            feed.search.as_deref(),
            Some("https://gutendex.com/books?languages=zh&search={searchTerms}")
        );
        let book = &feed.entries[0];
        assert_eq!(book.title, "西遊記");
        assert_eq!(book.author.as_deref(), Some("Wu, Cheng'en"));
        assert_eq!(
            book.cover.as_deref(),
            Some("https://www.gutenberg.org/cover.jpg")
        );
        let formats: Vec<&str> = book.downloads.iter().map(|d| d.format.as_str()).collect();
        assert_eq!(formats, ["EPUB", "TXT"], "EPUB first, no zips");
    }

    #[test]
    fn reads_wikisource_search_results() {
        let body = r#"{"continue": {"sroffset": 30}, "query": {"search": [
          {"title": "史記", "snippet": "<span class=\"searchmatch\">史</span>記"},
          {"title": "史記/卷001", "snippet": "a chapter"},
          {"title": "Les Misérables", "snippet": ""}]}}"#;
        let source = catalog("wikisource", "https://zh.wikisource.org/");
        let url = search_url(&wikisource_search(&source.url), "史記");
        assert!(
            url.starts_with("https://zh.wikisource.org/w/api.php?"),
            "{url}"
        );
        let feed = parse_page(&source, body, &url).unwrap();
        let titles: Vec<&str> = feed.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["史記", "Les Misérables"], "chapters are left out");
        assert_eq!(feed.entries[0].summary, "史記");
        let link = &feed.entries[0].downloads[0].url;
        assert!(
            link.starts_with("https://ws-export.wmcloud.org/?lang=zh&page="),
            "{link}"
        );
        assert!(link.ends_with("&format=epub"), "{link}");
        assert!(feed.next.as_deref().unwrap().contains("sroffset=30"));
        assert!(feed.search.is_some());
    }

    #[test]
    fn follows_a_redirect_written_as_json() {
        let url = "https://archive.org/services/opds/book/x?glob_pattern=*pdf";
        let reply = |status: u32, target: &str| {
            format!(r#"{{"status_code":{status},"_headers":{{"location":"{target}"}}}}"#)
        };
        let file = "https://archive.org/download/x/x.pdf";
        assert_eq!(
            wrapped_redirect(reply(302, file).as_bytes(), url).as_deref(),
            Some(file)
        );
        assert_eq!(wrapped_redirect(reply(404, file).as_bytes(), url), None);
        let elsewhere = reply(302, "https://evil.example/x.pdf");
        assert_eq!(
            wrapped_redirect(elsewhere.as_bytes(), url),
            None,
            "stays on the site"
        );
        let plain = reply(302, "http://archive.org/x.pdf");
        assert_eq!(wrapped_redirect(plain.as_bytes(), url), None);
        assert_eq!(
            wrapped_redirect(b"%PDF-1.7", url),
            None,
            "a real file is left alone"
        );
    }

    #[test]
    fn answers_a_digest_challenge() {
        let challenge = r#"Digest realm="calibre", qop="auth", nonce="abc123", algorithm=MD5"#;
        let answer =
            digest_answer(challenge, "http://h:8080/opds?x=1", "reader", Some("pw")).unwrap();
        assert!(answer.starts_with("Digest "), "{answer}");
        assert!(answer.contains(r#"username="reader""#), "{answer}");
        assert!(answer.contains(r#"uri="/opds?x=1""#), "{answer}");
        assert!(
            !answer.contains("pw\""),
            "the password itself never goes out: {answer}"
        );
        assert_eq!(
            digest_answer("Basic realm=\"x\"", "http://h/", "a", None),
            None
        );
    }

    #[test]
    fn a_wikisource_opens_on_its_search() {
        let home = wikisource_home(&catalog("wikisource", "https://de.wikisource.org/"));
        assert!(home.entries.is_empty());
        assert!(home.search.unwrap().contains("{searchTerms}"));
    }

    const LIST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>http://www.gutenberg.org/ebooks.opds/</id>
  <title>Project Gutenberg</title>
  <link rel="search" type="application/opensearchdescription+xml" href="https://www.gutenberg.org/catalog/osd-books.xml"/>
  <link rel="self" type="application/atom+xml;profile=opds-catalog" href="/ebooks/search.opds/?query=alice"/>
  <link rel="next" type="application/atom+xml;profile=opds-catalog" href="/ebooks/search.opds/?query=alice&amp;start_index=26"/>
  <link type="image/png" rel="http://opds-spec.org/image/thumbnail" href="data:image/png;base64,AAAA"/>
  <entry>
    <id>https://www.gutenberg.org/ebooks/16389.opds</id>
    <title>The Enchanted April</title>
    <content type="text">Elizabeth Von Arnim</content>
    <link type="application/atom+xml;profile=opds-catalog" rel="subsection" href="/ebooks/16389.opds"/>
    <link type="image/png" rel="http://opds-spec.org/image/thumbnail" href="data:image/png;base64,iVBOR"/>
  </entry>
  <entry>
    <id>urn:book:1</id>
    <title>Alice's Adventures</title>
    <author><name>Lewis Carroll</name></author>
    <content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>A girl falls</p><p>down a hole.</p></div></content>
    <link rel="http://opds-spec.org/image/thumbnail" type="image/jpeg" href="/cache/11.small.jpg"/>
    <link rel="http://opds-spec.org/image" type="image/jpeg" href="/cache/11.big.jpg"/>
    <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" title="EPUB (no images)" href="/ebooks/11.epub.noimages"/>
    <link rel="http://opds-spec.org/acquisition/open-access" type="application/x-mobipocket-ebook" href="https://cdn.example.org/11.mobi"/>
    <link rel="http://opds-spec.org/acquisition/buy" type="application/epub+zip" href="/buy/11"/>
    <link rel="http://opds-spec.org/acquisition" type="application/x-unknown" href="/ebooks/11.bin"/>
    <link rel="http://opds-spec.org/acquisition" type="application/pdf" href="javascript:alert(1)"/>
  </entry>
</feed>"#;

    #[test]
    fn reads_navigation_and_books() {
        let feed = parse_feed(
            LIST,
            "https://www.gutenberg.org/ebooks/search.opds/?query=alice",
        )
        .unwrap();
        assert_eq!(feed.title, "Project Gutenberg");
        assert_eq!(
            feed.next.as_deref(),
            Some("https://www.gutenberg.org/ebooks/search.opds/?query=alice&start_index=26")
        );
        assert_eq!(
            feed.search.as_deref(),
            Some("https://www.gutenberg.org/catalog/osd-books.xml")
        );
        let [folder, book] = &feed.entries[..] else {
            panic!("{:?}", feed.entries)
        };
        assert_eq!(
            folder.link.as_deref(),
            Some("https://www.gutenberg.org/ebooks/16389.opds")
        );
        assert_eq!(folder.summary, "Elizabeth Von Arnim");
        assert_eq!(folder.cover.as_deref(), Some("data:image/png;base64,iVBOR"));

        assert_eq!(book.author.as_deref(), Some("Lewis Carroll"));
        assert_eq!(book.summary, "A girl falls down a hole.");
        assert_eq!(
            book.cover.as_deref(),
            Some("https://www.gutenberg.org/cache/11.small.jpg")
        );
        assert_eq!(book.link, None, "a book is downloaded, not opened");
        let got: Vec<(&str, &str)> = book
            .downloads
            .iter()
            .map(|d| (d.format.as_str(), d.url.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("EPUB", "https://www.gutenberg.org/ebooks/11.epub.noimages"),
                ("MOBI", "https://cdn.example.org/11.mobi"),
            ],
            "no buying, unknown formats or strange addresses"
        );
        assert_eq!(book.downloads[0].title.as_deref(), Some("EPUB (no images)"));
    }

    #[test]
    fn search_templates() {
        let feed = parse_feed(
            r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>t</title>
               <link rel="search" type="application/opensearchdescription+xml" href="/osd.xml"/>
               <link rel="search" type="application/atom+xml" href="/opds/search?q={searchTerms}"/>
            </feed>"#,
            "http://192.168.1.2:8080/opds",
        )
        .unwrap();
        let template = feed.search.unwrap();
        assert_eq!(
            template,
            "http://192.168.1.2:8080/opds/search?q={searchTerms}"
        );
        assert_eq!(
            search_url(&template, " 三国 演义 "),
            "http://192.168.1.2:8080/opds/search?q=%E4%B8%89%E5%9B%BD+%E6%BC%94%E4%B9%89"
        );
        let osd = r#"<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
            <Url type="text/html" template="http://www.gutenberg.org/ebooks/search/?query={searchTerms}"/>
            <Url type="application/atom+xml" template="http://m.gutenberg.org/ebooks/search.opds/?query={searchTerms}"/>
        </OpenSearchDescription>"#;
        assert_eq!(
            parse_search_description(osd, "https://www.gutenberg.org/catalog/osd-books.xml")
                .unwrap(),
            "http://m.gutenberg.org/ebooks/search.opds/?query={searchTerms}"
        );
    }

    #[test]
    fn other_pages_are_refused() {
        assert!(parse_feed("<html><body>Log in</body></html>", "http://x/").is_err());
        assert!(parse_feed("{\"metadata\": {}}", "http://x/").is_err());
    }

    #[test]
    fn long_summaries_are_cut_and_tags_removed() {
        let long = "字".repeat(400);
        let xml = format!(
            r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>t</title>
               <entry><title>a</title><summary>{long}</summary></entry>
               <entry><title>b</title><content type="html">&lt;p&gt;plain &lt;b&gt;text&lt;/b&gt;&lt;/p&gt;</content></entry>
            </feed>"#
        );
        let feed = parse_feed(&xml, "http://x/").unwrap();
        assert_eq!(feed.entries[0].summary.chars().count(), MAX_SUMMARY + 1);
        assert_eq!(feed.entries[1].summary, "plain text");
    }

    #[test]
    fn passwords_stay_on_the_catalog_host() {
        assert!(same_host(
            "http://nas:8080/opds",
            "http://nas:8080/get/1.epub"
        ));
        assert!(!same_host(
            "http://nas:8080/opds",
            "http://nas:9000/get/1.epub"
        ));
        assert!(!same_host(
            "https://a.example/opds",
            "https://cdn.example/1.epub"
        ));
    }
}
