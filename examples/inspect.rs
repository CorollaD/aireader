//! Prints what the parser extracted from a file: `cargo run --example inspect -- <file>...`

use aireader::model::{Block, Content};

fn main() {
    for arg in std::env::args_os().skip(1) {
        let path = std::path::PathBuf::from(arg);
        println!("== {}", path.display());
        let start = std::time::Instant::now();
        let doc = match aireader::formats::open(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("   error: {e:#}");
                continue;
            }
        };
        println!(
            "   title: {}  author: {:?}  ({:?})",
            doc.title,
            doc.author,
            start.elapsed()
        );
        println!("   toc: {} entries", doc.toc.len());
        for e in doc.toc.iter().take(8) {
            println!(
                "     {}{}  -> {:?}",
                "  ".repeat(e.depth),
                e.title,
                e.location
            );
        }
        match &doc.content {
            Content::Flow(chapters) => {
                let blocks: usize = chapters.iter().map(|c| c.blocks.len()).sum();
                let images = chapters
                    .iter()
                    .flat_map(|c| &c.blocks)
                    .filter(|b| matches!(b, Block::Image(_)))
                    .count();
                println!(
                    "   chapters: {}  blocks: {blocks}  images: {images}",
                    chapters.len()
                );
                let sample: Vec<String> = chapters
                    .iter()
                    .flat_map(|c| &c.blocks)
                    .map(|b| b.text())
                    .filter(|t| t.chars().count() > 20)
                    .take(2)
                    .map(|t| t.chars().take(100).collect())
                    .collect();
                for s in sample {
                    println!("   | {s}");
                }
            }
            Content::Paged(src) => {
                println!(
                    "   pages: {}  aspect[0]: {:.3}",
                    src.page_count(),
                    src.page_aspect(0)
                );
                match src.render(0, 400) {
                    Ok(img) => println!("   rendered page 1 at {:?}", img.size),
                    Err(e) => println!("   render error: {e}"),
                }
            }
        }
    }
}
