//! Where synced files live: a WebDAV server, or a folder some other program keeps in sync
//! (iCloud Drive, Dropbox, OneDrive…). Paths are relative and use `/`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, Method, Response, StatusCode, Url};

const USER_AGENT: &str = concat!("AiReader/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const STALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<propfind xmlns="DAV:"><prop><resourcetype/><getetag/><getlastmodified/><getcontentlength/></prop></propfind>"#;

/// An entry of a folder. `tag` changes when the contents do.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteFile {
    pub name: String,
    pub dir: bool,
    pub tag: String,
}

pub enum Remote {
    Folder(PathBuf),
    WebDav(WebDav),
}

pub struct WebDav {
    client: Client,
    /// Ends with `/`.
    base: String,
    username: String,
    password: String,
}

impl WebDav {
    pub fn new(url: &str, folder: &str, username: &str, password: &str) -> Result<Self> {
        let mut base = url.trim().trim_end_matches('/').to_owned();
        if !matches!(
            Url::parse(&base).map(|u| u.scheme().to_owned()).as_deref(),
            Ok("http" | "https")
        ) {
            bail!(crate::t!("opds_bad_address"));
        }
        base.push('/');
        base.push_str(folder);
        base.push('/');
        Ok(Self {
            client: Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(CONNECT_TIMEOUT)
                .read_timeout(STALL_TIMEOUT)
                .build()?,
            base,
            username: username.to_owned(),
            password: password.to_owned(),
        })
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        build: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    ) -> Result<Response> {
        let url = format!("{}{path}", self.base);
        let mut request = self.client.request(method, url);
        if !self.username.is_empty() {
            request = request.basic_auth(&self.username, Some(&self.password));
        }
        build(request).send().await.map_err(|e| {
            log::warn!("sync request failed: {e}");
            anyhow!(crate::t!("sync_network"))
        })
    }
}

/// An error for a failed response.
fn failed(status: StatusCode) -> anyhow::Error {
    match status.as_u16() {
        401 | 403 => anyhow!(crate::t!("sync_auth")),
        507 => anyhow!(crate::t!("sync_full")),
        code => anyhow!(crate::t!("sync_failed", status = code)),
    }
}

/// Reads a WebDAV `multistatus` listing of `dir_path` (the folder's URL path).
pub fn parse_listing(xml: &str, dir_path: &str) -> Result<Vec<RemoteFile>> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| anyhow!("invalid listing: {e}"))?;
    let local = |n: &roxmltree::Node, name: &str| n.is_element() && n.tag_name().name() == name;
    let decode = |s: &str| crate::formats::percent_decode(s);
    let dir = decode(dir_path).trim_end_matches('/').to_owned();
    let mut files = Vec::new();
    for response in doc.descendants().filter(|n| local(n, "response")) {
        let Some(href) = response
            .descendants()
            .find(|n| local(n, "href"))
            .and_then(|n| n.text())
        else {
            continue;
        };
        let href = href.trim();
        // Some servers answer with full URLs.
        let path = Url::parse(href).map_or(href.to_owned(), |u| u.path().to_owned());
        let path = decode(&path);
        let path = path.trim_end_matches('/');
        if path == dir {
            continue;
        }
        let Some(name) = path.rsplit('/').next().filter(|n| !n.is_empty()) else {
            continue;
        };
        let text = |name: &str| {
            response
                .descendants()
                .find(|n| local(n, name))
                .and_then(|n| n.text())
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        let tag = match text("getetag") {
            etag if !etag.is_empty() => etag,
            _ => format!("{}-{}", text("getlastmodified"), text("getcontentlength")),
        };
        files.push(RemoteFile {
            name: name.to_owned(),
            dir: response.descendants().any(|n| local(&n, "collection")),
            tag,
        });
    }
    Ok(files)
}

impl Remote {
    /// The entries of a folder; a folder that doesn't exist yet is empty.
    pub async fn list(&self, dir: &str) -> Result<Vec<RemoteFile>> {
        match self {
            Remote::Folder(root) => {
                let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
                    return Ok(Vec::new());
                };
                let mut files = Vec::new();
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let meta = entry.metadata()?;
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_millis());
                    files.push(RemoteFile {
                        name,
                        dir: meta.is_dir(),
                        tag: format!("{modified}-{}", meta.len()),
                    });
                }
                Ok(files)
            }
            Remote::WebDav(dav) => {
                let path = if dir.is_empty() {
                    String::new()
                } else {
                    format!("{dir}/")
                };
                let response = dav
                    .send(Method::from_bytes(b"PROPFIND")?, &path, |r| {
                        r.header("Depth", "1")
                            .header(reqwest::header::CONTENT_TYPE, "application/xml")
                            .body(PROPFIND_BODY)
                    })
                    .await?;
                let status = response.status();
                if status == StatusCode::NOT_FOUND {
                    return Ok(Vec::new());
                }
                if !status.is_success() {
                    return Err(failed(status));
                }
                let url_path = response.url().path().to_owned();
                let xml = response.text().await?;
                parse_listing(&xml, &url_path)
            }
        }
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>> {
        match self {
            Remote::Folder(root) => Ok(std::fs::read(root.join(path))?),
            Remote::WebDav(dav) => {
                let response = dav.send(Method::GET, path, |r| r).await?;
                if !response.status().is_success() {
                    return Err(failed(response.status()));
                }
                Ok(response.bytes().await?.to_vec())
            }
        }
    }

    pub async fn write(&self, path: &str, bytes: Vec<u8>) -> Result<()> {
        match self {
            Remote::Folder(root) => {
                let dest = root.join(path);
                // Whole files only: another program may be copying the folder at any moment.
                let temp = dest.with_extension("partial");
                std::fs::write(&temp, bytes)?;
                std::fs::rename(&temp, &dest)?;
                Ok(())
            }
            Remote::WebDav(dav) => {
                let response = dav.send(Method::PUT, path, |r| r.body(bytes)).await?;
                if !response.status().is_success() {
                    return Err(failed(response.status()));
                }
                Ok(())
            }
        }
    }

    /// Makes a folder; its parent must exist. A folder already there is fine.
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        match self {
            Remote::Folder(root) => Ok(std::fs::create_dir_all(root.join(path))?),
            Remote::WebDav(dav) => {
                let url = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}/")
                };
                let response = dav.send(Method::from_bytes(b"MKCOL")?, &url, |r| r).await?;
                match response.status().as_u16() {
                    // 405: it exists already; 301: the server added a slash.
                    200..=299 | 301 | 405 => Ok(()),
                    _ => Err(failed(response.status())),
                }
            }
        }
    }

    pub async fn remove(&self, path: &str) -> Result<()> {
        match self {
            Remote::Folder(root) => Ok(std::fs::remove_file(root.join(path))?),
            Remote::WebDav(dav) => {
                let response = dav.send(Method::DELETE, path, |r| r).await?;
                if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
                    return Ok(());
                }
                Err(failed(response.status()))
            }
        }
    }

    /// Uploads a file without holding it in memory.
    pub async fn upload(&self, path: &str, source: &Path) -> Result<()> {
        match self {
            Remote::Folder(root) => {
                let dest = root.join(path);
                let temp = dest.with_extension("partial");
                std::fs::copy(source, &temp)?;
                std::fs::rename(&temp, &dest)?;
                Ok(())
            }
            Remote::WebDav(dav) => {
                let file = tokio::fs::File::open(source).await?;
                let size = file.metadata().await?.len();
                // A length, not chunks: not every WebDAV server takes chunked uploads.
                let response = dav
                    .send(Method::PUT, path, |r| {
                        r.header(reqwest::header::CONTENT_LENGTH, size)
                            .body(reqwest::Body::from(file))
                    })
                    .await?;
                if !response.status().is_success() {
                    return Err(failed(response.status()));
                }
                Ok(())
            }
        }
    }

    /// Saves a file to `dest`, without holding it in memory.
    pub async fn download(&self, path: &str, dest: &Path) -> Result<()> {
        use std::io::Write;
        match self {
            Remote::Folder(root) => {
                std::fs::copy(root.join(path), dest).context("copy")?;
                Ok(())
            }
            Remote::WebDav(dav) => {
                let mut response = dav.send(Method::GET, path, |r| r).await?;
                if !response.status().is_success() {
                    return Err(failed(response.status()));
                }
                let mut out = std::fs::File::create(dest)?;
                while let Some(chunk) = response.chunk().await? {
                    out.write_all(&chunk)?;
                }
                Ok(())
            }
        }
    }

    /// Checks that files can be written, read back and removed.
    pub async fn check(&self) -> Result<()> {
        const PROBE: &str = "probe.txt";
        self.mkdir("").await?;
        self.write(PROBE, b"AiReader".to_vec()).await?;
        let back = self.read(PROBE).await?;
        self.remove(PROBE).await?;
        if back != b"AiReader" {
            bail!(crate::t!("sync_failed", status = 0));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_from_different_servers() {
        // Relative hrefs, the DAV prefix and escaped names.
        let nginx = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:">
          <D:response><D:href>/dav/AiReader/devices/</D:href>
            <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat></D:response>
          <D:response><D:href>/dav/AiReader/devices/ab12/</D:href>
            <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype>
            <D:getlastmodified>Sun, 01 Nov 2026 10:00:00 GMT</D:getlastmodified></D:prop></D:propstat></D:response>
          <D:response><D:href>/dav/AiReader/devices/a%20b.json</D:href>
            <D:propstat><D:prop><D:resourcetype/><D:getetag>"v1"</D:getetag></D:prop></D:propstat></D:response>
        </D:multistatus>"#;
        let files = parse_listing(nginx, "/dav/AiReader/devices/").unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!((files[0].name.as_str(), files[0].dir), ("ab12", true));
        assert_eq!((files[1].name.as_str(), files[1].dir), ("a b.json", false));
        assert_eq!(files[1].tag, "\"v1\"");

        // Full URLs, another prefix, and no ETag.
        let other = r#"<a:multistatus xmlns:a="DAV:">
          <a:response><a:href>https://dav.example.com/dav/AiReader/</a:href></a:response>
          <a:response><a:href>https://dav.example.com/dav/AiReader/library.json</a:href>
            <a:propstat><a:prop><a:getlastmodified>t</a:getlastmodified><a:getcontentlength>12</a:getcontentlength></a:prop></a:propstat></a:response>
        </a:multistatus>"#;
        let files = parse_listing(other, "/dav/AiReader/").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].tag, "t-12");
    }

    #[tokio::test]
    async fn a_folder_behaves_like_a_server() {
        let dir = std::env::temp_dir().join(format!("aireader-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let remote = Remote::Folder(dir.clone());
        assert!(remote.list("nothing").await.unwrap().is_empty());
        remote.check().await.unwrap();
        remote.mkdir("a").await.unwrap();
        remote.mkdir("a").await.unwrap();
        remote.write("a/x.json", b"one".to_vec()).await.unwrap();
        let first = remote.list("a").await.unwrap();
        assert_eq!(first[0].name, "x.json");
        assert_eq!(remote.read("a/x.json").await.unwrap(), b"one");
        remote.write("a/x.json", b"three".to_vec()).await.unwrap();
        assert_ne!(remote.list("a").await.unwrap()[0].tag, first[0].tag);
    }
}
