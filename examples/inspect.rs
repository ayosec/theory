//! Dump the content of a Theory book.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    /// Path of the book.
    book: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input = BufReader::new(File::open(&args.book)?);
    let mut book = theory::Book::load(input)?;

    println!("Number of pages: {}", book.num_pages());

    println!("Metadata:");
    for entry in book.metadata()? {
        println!("\t{:?}", entry?);
    }

    println!("Pages:");
    for page in book.pages() {
        let page = page?;
        println!("= {:?} (parent {:?}) =", page.id(), page.parent());

        println!("Metadata:");
        for entry in page.metadata() {
            println!("\t{:?}", entry);
        }

        let content = page.content();
        println!(
            "Content ({} bytes):\n{}",
            content.len(),
            content.escape_ascii()
        );

        println!();
    }

    Ok(())
}
