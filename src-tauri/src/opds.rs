//! OPDS 1.x catalogs: online libraries described by Atom feeds. A feed lists entries that lead to
//! other feeds, or books with links to their files. This module only reads feeds; fetching is
//! done by the commands.

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};

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
            .collect(),
        next: by_rel("next").and_then(|h| absolute(&base, h)),
        search,
    })
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
