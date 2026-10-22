//! End-to-end parser tests on small files generated at test time.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use aireader::formats::{self, Format};
use aireader::model::{Block, Content, Document, Location};
use zip::write::SimpleFileOptions;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aireader-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in entries {
        let opts = if *name == "mimetype" {
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)
        } else {
            SimpleFileOptions::default()
        };
        zip.start_file(*name, opts).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

fn chapters(doc: &Document) -> &[aireader::model::Chapter] {
    match &doc.content {
        Content::Flow(c) => c,
        Content::Paged(_) => panic!("expected reflowable content"),
    }
}

#[test]
fn epub_spine_toc_and_images() {
    let dir = temp_dir("epub");
    let path = dir.join("book.epub");
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>测试之书</dc:title><dc:creator>作者</dc:creator><dc:identifier id="id">x</dc:identifier>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="empty" href="text/empty.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="img" href="images/pic%201.png" media-type="image/png"/>
  </manifest>
  <spine toc="ncx"><itemref idref="c1"/><itemref idref="empty"/><itemref idref="c2"/></spine>
</package>"#;
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap>
  <navPoint id="n1" playOrder="1"><navLabel><text>第一章</text></navLabel><content src="text/ch1.xhtml"/>
    <navPoint id="n1a" playOrder="2"><navLabel><text>小节</text></navLabel><content src="text/ch1.xhtml#sec"/></navPoint>
  </navPoint>
  <navPoint id="n2" playOrder="3"><navLabel><text>第二章</text></navLabel><content src="text/ch2.xhtml"/></navPoint>
</navMap></ncx>"#;
    let ch1 = r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><body>
        <h1>Chapter One</h1><p>第一段<b>粗体</b></p><img src="../images/pic%201.png" alt="pic"/>
        <h2 id="sec">Section</h2><p>more</p></body></html>"#;
    let ch2 = r#"<html><body><p>second chapter</p></body></html>"#;
    let empty = r#"<html><body></body></html>"#;
    let container = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
      <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let img = png(8, 4);
    write_zip(
        &path,
        &[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/toc.ncx", ncx.as_bytes()),
            ("OEBPS/text/ch1.xhtml", ch1.as_bytes()),
            ("OEBPS/text/empty.xhtml", empty.as_bytes()),
            ("OEBPS/text/ch2.xhtml", ch2.as_bytes()),
            ("OEBPS/images/pic 1.png", &img),
        ],
    );

    let doc = formats::open(&path).unwrap();
    assert_eq!(doc.title, "测试之书");
    assert_eq!(doc.author.as_deref(), Some("作者"));
    let ch = chapters(&doc);
    assert_eq!(ch.len(), 2, "empty spine item is dropped");
    assert_eq!(ch[0].title, "第一章");
    assert!(
        ch[0]
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Image(i) if i.bytes.len() == img.len()))
    );
    let toc: Vec<_> = doc
        .toc
        .iter()
        .map(|e| (e.title.as_str(), e.depth, e.location))
        .collect();
    assert_eq!(
        toc,
        [
            (
                "第一章",
                0,
                Location::Flow {
                    chapter: 0,
                    block: 0
                }
            ),
            (
                "小节",
                1,
                Location::Flow {
                    chapter: 0,
                    block: 3
                }
            ),
            (
                "第二章",
                0,
                Location::Flow {
                    chapter: 1,
                    block: 0
                }
            ),
        ]
    );
}

#[test]
fn docx_headings_runs_lists_tables_images() {
    let dir = temp_dir("docx");
    let path = dir.join("doc.docx");
    let w = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
    let document = format!(
        r#"<?xml version="1.0"?><w:document {w}><w:body>
        <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Intro</w:t></w:r></w:p>
        <w:p><w:r><w:t xml:space="preserve">plain </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:rPr><w:b w:val="0"/></w:rPr><w:t> not</w:t></w:r></w:p>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>item</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Second</w:t></w:r></w:p>
        <w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
        <w:p><w:r><w:drawing><a:graphic><a:graphicData><a:blip r:embed="rId5"/></a:graphicData></a:graphic></w:drawing></w:r></w:p>
        </w:body></w:document>"#
    );
    let styles = format!(
        r#"<w:styles {w}><w:style w:styleId="Heading1"><w:name w:val="heading 1"/></w:style></w:styles>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId5" Type="image" Target="media/image1.png"/></Relationships>"#;
    let core = r#"<cp:coreProperties xmlns:cp="x" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Doc Title</dc:title></cp:coreProperties>"#;
    write_zip(
        &path,
        &[
            ("word/document.xml", document.as_bytes()),
            ("word/styles.xml", styles.as_bytes()),
            ("word/_rels/document.xml.rels", rels.as_bytes()),
            ("word/media/image1.png", &png(2, 2)),
            ("docProps/core.xml", core.as_bytes()),
        ],
    );

    let doc = formats::open(&path).unwrap();
    assert_eq!(doc.title, "Doc Title");
    let ch = chapters(&doc);
    assert_eq!(
        ch.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
        ["Intro", "Second"]
    );
    let Block::Paragraph(spans) = &ch[0].blocks[1] else {
        panic!("{:?}", ch[0].blocks[1])
    };
    assert_eq!(
        spans
            .iter()
            .map(|s| (s.text.as_str(), s.style.bold))
            .collect::<Vec<_>>(),
        [("plain ", false), ("bold", true), (" not", false)]
    );
    assert!(matches!(&ch[0].blocks[2], Block::ListItem { depth: 1, .. }));
    assert!(ch[1].blocks[1].text().starts_with('A') && ch[1].blocks[1].text().contains('B'));
    assert!(matches!(ch[1].blocks[2], Block::Image(_)));
    assert_eq!(doc.toc.len(), 2);
}

#[test]
fn cbz_natural_order_and_render() {
    let dir = temp_dir("cbz");
    let path = dir.join("comic.cbz");
    write_zip(
        &path,
        &[
            ("10.png", &png(10, 30)),
            ("2.png", &png(10, 20)),
            ("1.png", &png(10, 10)),
            ("__MACOSX/._1.png", b"junk"),
            ("notes.txt", b"x"),
        ],
    );
    let doc = formats::open(&path).unwrap();
    let Content::Paged(src) = &doc.content else {
        panic!()
    };
    assert_eq!(src.page_count(), 3);
    let img = src.render(2, 5).unwrap();
    assert_eq!(
        (img.width, img.height),
        (5, 15),
        "page 3 is 10.png, downscaled to width 5"
    );
    assert!((src.page_aspect(2) - 3.0).abs() < 1e-3);
    let img = src.render(0, 100).unwrap();
    assert_eq!((img.width, img.height), (10, 10), "never upscaled");
}

#[test]
fn fb2_zip_and_gbk_txt_and_sniffing() {
    let dir = temp_dir("misc");
    let fb2 = r#"<?xml version="1.0" encoding="utf-8"?><FictionBook><description><title-info><book-title>Zipped</book-title></title-info></description>
        <body><section><title><p>One</p></title><p>text</p></section></body></FictionBook>"#;
    let fb2_path = dir.join("book.fb2.zip");
    write_zip(&fb2_path, &[("book.fb2", fb2.as_bytes())]);
    assert_eq!(formats::detect(&fb2_path).unwrap(), Format::Fb2Zip);
    assert_eq!(formats::open(&fb2_path).unwrap().title, "Zipped");

    let txt_path = dir.join("小说.txt");
    let (gbk, _, _) = encoding_rs::GBK.encode("第一章 开始\n　　从前有座山，山里有座庙，庙里有个老和尚在给小和尚讲故事。\n第二章 继续\n　　故事讲的是什么呢？");
    std::fs::write(&txt_path, &gbk).unwrap();
    let doc = formats::open(&txt_path).unwrap();
    assert_eq!(doc.title, "小说");
    assert_eq!(chapters(&doc).len(), 2);
    assert_eq!(doc.toc[1].title, "第二章 继续");

    let unknown = dir.join("noext");
    std::fs::write(&unknown, b"%PDF-1.4\n").unwrap();
    assert_eq!(formats::detect(&unknown).unwrap(), Format::Pdf);
}

/// Minimal one-page PDF with an outline entry, offsets computed for a valid xref table.
fn minimal_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 600] /Contents 4 0 R /Resources << /Font << /F1 7 0 R >> >> >>",
        "<< /Length 42 >>\nstream\nBT /F1 24 Tf 30 500 Td (Hello World) Tj ET\nendstream",
        "<< /Type /Outlines /First 6 0 R /Last 6 0 R /Count 1 >>",
        "<< /Title (Start) /Parent 5 0 R /Dest [3 0 R /Fit] >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[test]
fn pdf_render_and_outline() {
    if let Err(e) = formats::pdf::pdfium() {
        eprintln!("skipping: {e}");
        return;
    }
    let dir = temp_dir("pdf");
    let path = dir.join("doc.pdf");
    std::fs::write(&path, minimal_pdf()).unwrap();
    let doc = formats::open(&path).unwrap();
    let Content::Paged(src) = &doc.content else {
        panic!()
    };
    assert_eq!(src.page_count(), 1);
    assert!((src.page_aspect(0) - 2.0).abs() < 1e-3);
    let img = src.render(0, 150).unwrap();
    assert_eq!((img.width, img.height), (150, 300));
    assert_eq!(img.rgba.len(), 150 * 300 * 4);
    assert_eq!(doc.toc.len(), 1);
    assert_eq!(doc.toc[0].title, "Start");
    assert_eq!(doc.toc[0].location, Location::Page { page: 0 });

    // The text layer: one run where the text was drawn (x = 30/300, baseline at 500 of 600).
    assert_eq!(
        src.page_text(0).as_deref().map(str::trim),
        Some("Hello World")
    );
    let runs = src.text_runs(0);
    assert_eq!(runs.len(), 1, "{runs:?}");
    let r = &runs[0];
    assert_eq!(r.text.trim(), "Hello World");
    assert!((r.x - 0.1).abs() < 0.02, "{r:?}");
    assert!(
        r.y > 0.12 && r.y < 0.18 && r.h > 0.01 && r.h < 0.06,
        "{r:?}"
    );
    assert!(r.w > 0.3 && r.w < 0.6, "{r:?}");
}
