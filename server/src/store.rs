//! Each account's private folder. Paths come from clients, so every one is checked before it
//! touches the disk: plain names joined by `/`, nothing hidden, nothing that climbs out.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use futures_util::{Stream, StreamExt};
use rand::RngExt;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub enum StoreError {
    BadPath,
    NotFound,
    TooLarge,
    QuotaFull,
    Io(std::io::Error),
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            StoreError::NotFound
        } else {
            StoreError::Io(e)
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    /// Changes whenever the contents do.
    pub tag: String,
}

/// Turns a client path into one under `root`. The empty path is `root` itself.
pub fn resolve(root: &Path, path: &str) -> Result<PathBuf, StoreError> {
    let mut out = root.to_path_buf();
    if path.is_empty() {
        return Ok(out);
    }
    if path.len() > 1024 {
        return Err(StoreError::BadPath);
    }
    for part in path.split('/') {
        let bad = part.is_empty()
            || part.starts_with('.')
            || part.len() > 255
            || part
                .chars()
                .any(|c| c == '\\' || c == ':' || c.is_control());
        if bad {
            return Err(StoreError::BadPath);
        }
        out.push(part);
    }
    Ok(out)
}

fn tag(meta: &std::fs::Metadata) -> String {
    let modified = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("{modified}-{}", meta.len())
}

/// A folder's entries; a folder that doesn't exist yet is empty.
pub async fn list(root: &Path, path: &str) -> Result<Vec<Entry>, StoreError> {
    let dir = resolve(root, path)?;
    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let meta = entry.metadata().await?;
        out.push(Entry {
            name,
            dir: meta.is_dir(),
            tag: tag(&meta),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Bytes used under `root`.
pub fn usage(root: &Path) -> u64 {
    fn walk(dir: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| match e.metadata() {
                Ok(m) if m.is_dir() => walk(&e.path()),
                Ok(m) => m.len(),
                Err(_) => 0,
            })
            .sum()
    }
    walk(root)
}

pub struct Limits {
    /// Largest single file.
    pub max_file: u64,
    /// Most bytes an account may hold; 0 for no limit.
    pub quota: u64,
}

/// Writes a whole file from a stream of chunks. The file only appears once complete, so a
/// reader never sees half of it, and an upload that fails leaves the old file in place.
pub async fn write<S, E>(
    root: &Path,
    path: &str,
    body: S,
    limits: &Limits,
) -> Result<(), StoreError>
where
    S: Stream<Item = Result<axum::body::Bytes, E>> + Unpin,
{
    let dest = resolve(root, path)?;
    if path.is_empty() || dest.is_dir() {
        return Err(StoreError::BadPath);
    }
    let parent = dest.parent().ok_or(StoreError::BadPath)?;
    tokio::fs::create_dir_all(parent).await?;
    // What the account may still add, counting the file being replaced as free.
    let room = if limits.quota > 0 {
        let replaced = tokio::fs::metadata(&dest).await.map_or(0, |m| m.len());
        let root_owned = root.to_path_buf();
        let used = tokio::task::spawn_blocking(move || usage(&root_owned))
            .await
            .unwrap_or(0);
        Some(limits.quota.saturating_sub(used.saturating_sub(replaced)))
    } else {
        None
    };

    let suffix: u64 = rand::rng().random();
    let name = dest
        .file_name()
        .ok_or(StoreError::BadPath)?
        .to_string_lossy();
    let temp = parent.join(format!(".{name}.{suffix:016x}.partial"));
    let result = async {
        let mut file = tokio::fs::File::create(&temp).await?;
        let mut written: u64 = 0;
        let mut body = body;
        while let Some(chunk) = body.next().await {
            let chunk = chunk
                .map_err(|_| StoreError::Io(std::io::Error::other("the upload was interrupted")))?;
            written += chunk.len() as u64;
            if written > limits.max_file {
                return Err(StoreError::TooLarge);
            }
            if room.is_some_and(|room| written > room) {
                return Err(StoreError::QuotaFull);
            }
            file.write_all(&chunk).await?;
        }
        file.sync_all().await?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            tokio::fs::rename(&temp, &dest).await?;
            Ok(())
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&temp).await;
            Err(e)
        }
    }
}

pub async fn mkdir(root: &Path, path: &str) -> Result<(), StoreError> {
    tokio::fs::create_dir_all(resolve(root, path)?).await?;
    Ok(())
}

/// Removes a file. One that isn't there is already removed.
pub async fn remove(root: &Path, path: &str) -> Result<(), StoreError> {
    let target = resolve(root, path)?;
    if path.is_empty() || target.is_dir() {
        return Err(StoreError::BadPath);
    }
    match tokio::fs::remove_file(target).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_folder() {
        let root = Path::new("/data/files/1");
        assert_eq!(resolve(root, "").unwrap(), root);
        assert_eq!(
            resolve(root, "devices/ab12/library.json").unwrap(),
            root.join("devices/ab12/library.json")
        );
        for bad in [
            "..",
            "../2/x",
            "a/../../x",
            "/etc/passwd",
            "a//b",
            "a/",
            ".hidden",
            "a/.x.partial",
            "a\\b",
            "c:x",
            "a\nb",
        ] {
            assert!(
                matches!(resolve(root, bad), Err(StoreError::BadPath)),
                "{bad:?} should be refused"
            );
        }
    }

    fn chunks(
        parts: &[&'static [u8]],
    ) -> impl Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Unpin {
        futures_util::stream::iter(
            parts
                .iter()
                .map(|p| Ok(axum::body::Bytes::from_static(p)))
                .collect::<Vec<_>>(),
        )
    }

    #[tokio::test]
    async fn writes_whole_files_within_limits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let roomy = Limits {
            max_file: 100,
            quota: 0,
        };
        write(root, "a/b.json", chunks(&[b"he", b"llo"]), &roomy)
            .await
            .unwrap();
        assert_eq!(std::fs::read(root.join("a/b.json")).unwrap(), b"hello");
        let listed = list(root, "a").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "b.json");
        assert!(list(root, "missing").await.unwrap().is_empty());

        let small = Limits {
            max_file: 4,
            quota: 0,
        };
        assert!(matches!(
            write(root, "a/b.json", chunks(&[b"too", b"long"]), &small).await,
            Err(StoreError::TooLarge)
        ));
        // The old file survives, and no temporary file is left behind.
        assert_eq!(std::fs::read(root.join("a/b.json")).unwrap(), b"hello");
        assert_eq!(std::fs::read_dir(root.join("a")).unwrap().count(), 1);

        // Replacing a file counts only the difference against the quota.
        let tight = Limits {
            max_file: 100,
            quota: 8,
        };
        write(root, "a/b.json", chunks(&[b"12345678"]), &tight)
            .await
            .unwrap();
        assert!(matches!(
            write(root, "a/c.json", chunks(&[b"x"]), &tight).await,
            Err(StoreError::QuotaFull)
        ));

        remove(root, "a/b.json").await.unwrap();
        remove(root, "a/b.json").await.unwrap();
        assert!(list(root, "a").await.unwrap().is_empty());
    }
}
