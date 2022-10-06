//! Write a Theory book with the files given in the command line.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    /// File to write the book.
    #[arg(short, long)]
    book: PathBuf,

    /// Title of the book.
    #[arg(short, long)]
    title: Option<String>,

    /// Files to include in the book.
    pages: Vec<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();

    let mut book = theory::Book::builder();

    if let Some(title) = args.title.take() {
        book.add_metadata(theory::MetadataEntry::Title(title));
    }

    for page in &args.pages {
        book.new_page(page.display().to_string())
            .set_content(fs::read(&page)?);
    }

    // Write the book in the file.
    let output = BufWriter::new(File::create(&args.book)?);
    book.dump(output)?;

    Ok(())
}
