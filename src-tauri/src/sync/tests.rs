//! Two libraries sharing a sync folder, as two devices would.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::remote::Remote;
use super::*;
use crate::annotations::{AnnotationStore, Annotations};
use crate::cards::NewCard;
use crate::library::{BookPatch, stage};
use crate::model::Location;

struct Device {
    library: Mutex<Library>,
    dir: PathBuf,
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aireader-sync-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn device(root: &Path, name: &str) -> Device {
    let dir = root.join(name);
    Device {
        library: Mutex::new(Library::open(&dir.join("lib")).unwrap()),
        dir,
    }
}

impl Device {
    fn lib(&self) -> std::sync::MutexGuard<'_, Library> {
        self.library.lock().unwrap()
    }

    /// Imports a text file; returns the book's id.
    fn import(&self, name: &str, text: &str) -> String {
        std::fs::create_dir_all(&self.dir).unwrap();
        let src = self.dir.join(name);
        std::fs::write(&src, text).unwrap();
        let mut lib = self.lib();
        let staged = stage(lib.root(), &src).unwrap();
        let id = staged.sha256[..16].to_owned();
        let prepared = Library::prepare(staged).unwrap();
        lib.add(prepared, None).unwrap();
        lib.save().unwrap();
        id
    }

    fn annotations(&self, id: &str) -> Annotations {
        AnnotationStore::new(self.lib().db()).load(id).unwrap()
    }

    fn save_annotations(&self, id: &str, ann: &Annotations) {
        AnnotationStore::new(self.lib().db()).save(id, ann).unwrap();
    }
}

fn highlight(id: &str, updated: u64) -> Highlight {
    Highlight {
        id: id.into(),
        chapter: 0,
        start_block: 0,
        start: 0,
        end_block: 0,
        end: 2,
        text: "正文".into(),
        color: "yellow".into(),
        note: None,
        created: updated,
        updated,
    }
}

async fn sync(d: &Device, remote: &Remote) -> Outcome {
    run(&d.library, remote, false, false, |_| {}).await.unwrap()
}

fn setup(name: &str) -> (Device, Device, Remote) {
    let root = temp(name);
    let remote = Remote::Folder(root.join("cloud"));
    (device(&root, "a"), device(&root, "b"), remote)
}

#[tokio::test]
async fn a_second_device_gets_the_shelf_and_reading_data() {
    let (a, b, remote) = setup("first");
    let id = a.import("书.txt", "第一章\n正文一\n第二章\n正文二");
    {
        let mut lib = a.lib();
        lib.set_progress(
            &id,
            Location::Flow {
                chapter: 1,
                block: 1,
            },
            0.6,
        )
        .unwrap();
        lib.add_reading_time(&id, 600, None).unwrap();
        lib.update(&id, serde_json::from_str(r#"{"group":"小说"}"#).unwrap())
            .unwrap();
        lib.save().unwrap();
        crate::stats::record(&lib.db().conn(), "2026-03-10", &id, "书", 600).unwrap();
    }
    a.save_annotations(
        &id,
        &Annotations {
            highlights: vec![highlight("h1", 100)],
            ..Default::default()
        },
    );
    cards::add(
        &a.lib().db().conn(),
        NewCard {
            id: "c1".into(),
            book_id: Some(id.clone()),
            book_title: "书".into(),
            front: "词".into(),
            back: "义".into(),
            context: String::new(),
            location: None,
        },
        100,
    )
    .unwrap();

    assert_eq!(sync(&a, &remote).await.devices, 0);
    let got = sync(&b, &remote).await;
    assert_eq!(got.devices, 1);
    assert!(got.merged.shelf);

    let lib = b.lib();
    let book = lib.get(&id).unwrap();
    assert!(book.synced);
    assert_eq!(book.progress, 0.6);
    assert_eq!(book.group.as_deref(), Some("小说"));
    assert_eq!(book.read_seconds, 600);
    assert_eq!(lib.groups(), ["小说"]);
    assert!(lib.books()[0].missing, "the file hasn't been fetched");
    let conn = lib.db().conn();
    assert_eq!(cards::get(&conn, "c1").unwrap().back, "义");
    let report = crate::stats::report(&conn, "2026-03-10", 2026).unwrap();
    assert_eq!(report.today, 600);
    drop(conn);
    drop(lib);
    assert_eq!(b.annotations(&id).highlights.len(), 1);
}

#[tokio::test]
async fn nothing_is_uploaded_when_nothing_changed() {
    let (a, b, remote) = setup("quiet");
    a.import("a.txt", "第一章\n甲");
    assert!(sync(&a, &remote).await.uploaded > 0);
    assert_eq!(sync(&a, &remote).await.uploaded, 0);
    sync(&b, &remote).await;
    sync(&a, &remote).await;
    // Once both have seen everything, neither has anything to say.
    assert_eq!(sync(&b, &remote).await.uploaded, 0);
    assert_eq!(sync(&a, &remote).await.uploaded, 0);
}

#[tokio::test]
async fn both_sides_keep_their_own_changes() {
    let (a, b, remote) = setup("merge");
    let id = a.import("a.txt", "第一章\n正文");
    sync(&a, &remote).await;
    sync(&b, &remote).await;

    a.save_annotations(
        &id,
        &Annotations {
            highlights: vec![highlight("from-a", 100)],
            ..Default::default()
        },
    );
    b.save_annotations(
        &id,
        &Annotations {
            highlights: vec![highlight("from-b", 100)],
            ..Default::default()
        },
    );
    a.lib().add_reading_time(&id, 100, None).unwrap();
    a.lib().save().unwrap();
    b.lib().add_reading_time(&id, 50, None).unwrap();
    b.lib().save().unwrap();

    for _ in 0..2 {
        sync(&a, &remote).await;
        sync(&b, &remote).await;
    }
    for d in [&a, &b] {
        let mut ids: Vec<String> = d
            .annotations(&id)
            .highlights
            .into_iter()
            .map(|h| h.id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["from-a", "from-b"]);
        assert_eq!(
            d.lib().get(&id).unwrap().read_seconds,
            150,
            "time adds up, once"
        );
    }
}

#[tokio::test]
async fn the_newest_edit_wins() {
    let (a, b, remote) = setup("newest");
    let id = a.import("a.txt", "第一章\n正文");
    a.save_annotations(
        &id,
        &Annotations {
            highlights: vec![highlight("h", 100)],
            ..Default::default()
        },
    );
    sync(&a, &remote).await;
    sync(&b, &remote).await;

    let older = Highlight {
        note: Some("旧".into()),
        ..highlight("h", 200)
    };
    let newer_note = Highlight {
        note: Some("新".into()),
        ..highlight("h", 300)
    };
    a.save_annotations(
        &id,
        &Annotations {
            highlights: vec![older],
            ..Default::default()
        },
    );
    b.save_annotations(
        &id,
        &Annotations {
            highlights: vec![newer_note],
            ..Default::default()
        },
    );
    for _ in 0..2 {
        sync(&a, &remote).await;
        sync(&b, &remote).await;
    }
    for d in [&a, &b] {
        assert_eq!(d.annotations(&id).highlights[0].note.as_deref(), Some("新"));
    }
}

#[tokio::test]
async fn deletions_reach_other_devices_and_stay_deleted() {
    let (a, b, remote) = setup("delete");
    let id = a.import("a.txt", "第一章\n正文");
    a.save_annotations(
        &id,
        &Annotations {
            highlights: vec![highlight("h", 100)],
            ..Default::default()
        },
    );
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    assert_eq!(b.annotations(&id).highlights.len(), 1);

    a.save_annotations(&id, &Annotations::default());
    sync(&a, &remote).await;
    let got = sync(&b, &remote).await;
    assert!(b.annotations(&id).highlights.is_empty());
    assert_eq!(
        got.merged.books,
        std::slice::from_ref(&id),
        "an open book must hear of it"
    );
    // b's next publish can't bring it back.
    sync(&b, &remote).await;
    sync(&a, &remote).await;
    assert!(a.annotations(&id).highlights.is_empty());
}

#[tokio::test]
async fn removing_a_book_removes_it_everywhere() {
    let (a, b, remote) = setup("remove");
    let id = a.import("a.txt", "第一章\n正文");
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    assert!(b.lib().get(&id).is_some());
    a.lib().remove(std::slice::from_ref(&id));
    a.lib().save().unwrap();
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    assert!(b.lib().get(&id).is_none());
    sync(&b, &remote).await;
    sync(&a, &remote).await;
    assert!(a.lib().get(&id).is_none(), "and it doesn't come back");
}

#[tokio::test]
async fn a_book_edited_after_removal_survives() {
    let (a, b, remote) = setup("resurrect");
    let id = a.import("a.txt", "第一章\n正文");
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    // Removed on a long ago, but b has read it since.
    crate::db::tombstone(&a.lib().db().conn(), "book", &id, 1).unwrap();
    b.lib()
        .update(
            &id,
            BookPatch {
                title: Some("新名".into()),
                ..Default::default()
            },
        )
        .unwrap();
    b.lib().save().unwrap();
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    assert_eq!(b.lib().get(&id).unwrap().title, "新名");
}

#[tokio::test]
async fn card_reviews_follow_the_latest_answer() {
    let (a, b, remote) = setup("cards");
    let card = |id: &str| NewCard {
        id: id.into(),
        book_id: None,
        book_title: "书".into(),
        front: id.into(),
        back: "义".into(),
        context: String::new(),
        location: None,
    };
    cards::add(&a.lib().db().conn(), card("x"), 1_000).unwrap();
    cards::add(&a.lib().db().conn(), card("gone"), 1_000).unwrap();
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    cards::review(&b.lib().db().conn(), "x", 3, 2_000).unwrap();
    cards::delete(&b.lib().db().conn(), "gone").unwrap();
    sync(&b, &remote).await;
    sync(&a, &remote).await;
    let conn_lib = a.lib();
    let conn = conn_lib.db().conn();
    assert_eq!(cards::get(&conn, "x").unwrap().reps, 1);
    assert!(cards::get(&conn, "gone").is_err());
    assert_eq!(cards::sync_reviews(&conn).unwrap().len(), 1);
}

#[tokio::test]
async fn book_files_are_fetched_on_request_and_checked() {
    let (a, b, remote) = setup("files");
    let id = a.import("a.txt", "第一章\n正文");
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    assert!(
        fetch_book(&b.library, &remote, &id).await.is_err(),
        "not uploaded yet"
    );

    run(&a.library, &remote, true, false, |_| {}).await.unwrap();
    fetch_book(&b.library, &remote, &id).await.unwrap();
    assert!(!b.lib().books()[0].missing);

    // A file that isn't what the record says is refused.
    let c = device(&b.dir, "c");
    sync(&c, &remote).await;
    remote
        .write(&format!("files/{id}.book"), b"tampered".to_vec())
        .await
        .unwrap();
    assert!(fetch_book(&c.library, &remote, &id).await.is_err());
    assert!(c.lib().books()[0].missing);
}

#[tokio::test]
async fn importing_a_known_book_fills_in_its_file() {
    let (a, b, remote) = setup("import");
    let text = "第一章\n正文";
    let id = a.import("a.txt", text);
    sync(&a, &remote).await;
    sync(&b, &remote).await;
    assert!(b.lib().books()[0].missing);
    std::fs::create_dir_all(&b.dir).unwrap();
    let src = b.dir.join("a.txt");
    std::fs::write(&src, text).unwrap();
    let staged = stage(b.lib().root(), &src).unwrap();
    b.lib().fill_missing(staged).unwrap();
    assert!(!b.lib().books()[0].missing);
    assert_eq!(b.lib().books()[0].id, id);
}

#[tokio::test]
async fn a_cleared_server_gets_everything_again() {
    let (a, _b, remote) = setup("cleared");
    let id = a.import("a.txt", "第一章\n正文");
    a.save_annotations(
        &id,
        &Annotations {
            highlights: vec![highlight("h", 100)],
            ..Default::default()
        },
    );
    sync(&a, &remote).await;
    let Remote::Folder(root) = &remote else {
        unreachable!()
    };
    std::fs::remove_dir_all(root).unwrap();
    assert!(sync(&a, &remote).await.uploaded > 0);
    let device = crate::stats::device_id(&a.lib().db().conn()).unwrap();
    assert!(
        root.join("devices")
            .join(&device)
            .join(book_file(&id))
            .exists()
    );

    // A book left with no notes has no file.
    a.save_annotations(&id, &Annotations::default());
    sync(&a, &remote).await;
    assert!(
        !root
            .join("devices")
            .join(&device)
            .join(book_file(&id))
            .exists()
    );
}

#[tokio::test]
async fn covers_travel_and_appear_when_they_arrive() {
    let (a, b, remote) = setup("covers");
    let id = a.import("a.txt", "第一章\n正文");
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(4, 4)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    std::fs::write(a.lib().cover_path(&id), jpeg.get_ref()).unwrap();
    a.lib().cover_fetched(&id);
    sync(&a, &remote).await;

    let notes = Mutex::new(Vec::new());
    run(&b.library, &remote, false, false, |m| {
        notes.lock().unwrap().push(m.shelf)
    })
    .await
    .unwrap();
    assert_eq!(
        *notes.lock().unwrap(),
        [true, true],
        "the book, then its cover"
    );
    {
        let lib = b.lib();
        assert!(lib.get(&id).unwrap().has_cover);
        assert!(lib.cover_path(&id).exists());
    }

    // Anything else under that name stays out.
    let (c, d, other) = setup("covers-bad");
    let id = c.import("a.txt", "第一章\n正文");
    sync(&c, &other).await;
    other
        .write(&format!("covers/{id}.jpg"), b"<script>".to_vec())
        .await
        .unwrap();
    sync(&d, &other).await;
    assert!(!d.lib().get(&id).unwrap().has_cover);
    assert!(!d.lib().cover_path(&id).exists());
}

#[tokio::test]
async fn a_half_written_file_is_retried_next_time() {
    let (a, b, remote) = setup("partial");
    let id = a.import("a.txt", "第一章\n正文");
    sync(&a, &remote).await;
    let device = crate::stats::device_id(&a.lib().db().conn()).unwrap();
    let path = format!("devices/{device}/{LIBRARY}");
    let whole = remote.read(&path).await.unwrap();
    remote
        .write(&path, whole[..whole.len() / 2].to_vec())
        .await
        .unwrap();
    sync(&b, &remote).await;
    assert!(b.lib().get(&id).is_none());
    remote.write(&path, whole).await.unwrap();
    sync(&b, &remote).await;
    assert!(b.lib().get(&id).is_some());
}

#[test]
fn records_from_the_folder_cannot_escape_the_library() {
    let book: Book = serde_json::from_value(serde_json::json!({
        "id": "0123456789abcdef",
        "title": "t", "author": null, "format": "EPUB",
        "fileName": "a.epub", "size": 1,
        "sha256": format!("0123456789abcdef{}", "0".repeat(48)),
        "hasCover": false, "added": 0, "lastRead": null,
        "progress": 0.0, "position": null,
    }))
    .unwrap();
    assert!(valid_book(&book));
    let with = |f: &dyn Fn(&mut Book)| {
        let mut b = book.clone();
        f(&mut b);
        valid_book(&b)
    };
    assert!(!with(&|b| b.file_name = "../../evil".into()));
    assert!(!with(&|b| b.file_name = "..\\evil".into()));
    assert!(!with(&|b| b.file_name = ".hidden".into()));
    assert!(!with(&|b| b.id = "../../../etc/pass".into()));
    assert!(
        !with(&|b| b.id = "fedcba9876543210".into()),
        "id must match the hash"
    );
    assert!(valid_device("0123456789abcdef"));
    assert!(!valid_device(".."));
    assert!(!valid_device("devices"));
}

#[test]
fn ties_go_the_same_way_on_every_device() {
    let (x, y) = ("a".to_owned(), "b".to_owned());
    assert_eq!(newer((5, &x), (5, &y)), !newer((5, &y), (5, &x)));
    assert!(newer((6, &x), (5, &y)));
}
