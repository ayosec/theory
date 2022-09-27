//! Version 1 of the book files.

use std::io::{Read, Seek, SeekFrom, Write};

use super::Error;
use crate::builder::BookBuilder;
use crate::persistence::datablock::DataBlocksReader;
use crate::{metadata, page, Book};

use endiannezz::Io;

/// Magic string for this version.
///
/// Byte `89` is used to help to identify this file as binary data (the same
/// byte used by PNG).
///
/// Byte `01` can be used to identify the version number.
pub(super) const MAGIC: &[u8; super::MAGIC_SIZE] = b"\x89\x01THRPKG";

#[derive(Io)]
#[endian(big)]
struct Header {
    num_pages: u32,
    metadata_pos: u32,
    pages_pos: u32,
    fts_pos: u32,
}

pub(super) fn load<I>(mut input: I) -> Result<crate::Book<I>, Error>
where
    I: Read + Seek,
{
    let header = Header::read(&mut input)?;

    let num_pages = header.num_pages.try_into()?;
    let page_index = page::Index::new(&mut input, num_pages, header.pages_pos.into())?;

    let book = Book {
        data_blocks: DataBlocksReader::new(input),
        num_pages,
        metadata_pos: header.metadata_pos.try_into()?,
        page_index,
    };

    Ok(book)
}

pub(super) fn dump<O>(mut output: O, book: &BookBuilder) -> Result<(), Error>
where
    O: Write + Seek,
{
    macro_rules! to_u32 {
        ($v:expr) => {
            u32::try_from($v).map_err(|_| Error::TooManyPages)?
        };
    }

    let mut header = Header {
        num_pages: to_u32!(book.pages.len()),
        metadata_pos: !0,
        pages_pos: !0,
        fts_pos: !0,
    };

    let beginning = output.stream_position()?;

    // The magic number must be at the beginning of the stream.
    output.write_all(MAGIC)?;

    // Write the (incomplete) header data to reserve its space in the stream.
    header.write(&mut output)?;

    // The metadata table.
    header.metadata_pos = to_u32!(output.stream_position()? - beginning);
    metadata::dump(&mut output, &book.metadata)?;

    // The pages table.
    let page_pos = page::persistence::dump_pages(&mut output, &book.pages)?;
    header.pages_pos = to_u32!(page_pos - beginning);

    // TODO Write a table for the FTS index.

    // Write the final header.
    output.seek(SeekFrom::Start(beginning + MAGIC.len() as u64))?;
    header.write(&mut output)?;

    Ok(())
}

#[test]
fn dump_and_load() {
    use crate::{Book, MetadataEntry};
    use std::io::Cursor;

    let metadata = [
        MetadataEntry::Title("Theory Example".into()),
        MetadataEntry::Date(1234),
    ];

    let mut builder = Book::builder();

    for entry in &metadata {
        builder.add_metadata(entry.clone());
    }

    let page1 = builder
        .new_page("First")
        .set_keywords("abc, def")
        .set_description("abcdef")
        .set_content("- 1 -")
        .clone();

    let page2 = builder
        .new_page("Second")
        .set_parent(page1.id())
        .set_keywords("abc, def")
        .set_description("abcdef")
        .set_content("- 2 -")
        .clone();

    let mut buffer: Vec<u8> = Vec::new();
    builder
        .dump(Cursor::new(&mut buffer))
        .expect("BookBuilder::dump");

    let mut book = Book::load(Cursor::new(buffer)).unwrap();

    // Check metadata.
    let pkg_metadata: Vec<_> = book
        .metadata()
        .expect("Invalid metadata")
        .map(|entry| entry.expect("Invalid entry"))
        .collect();

    assert_eq!(pkg_metadata[..], metadata[..]);

    // Load a single page.
    let found_page = book.get_page_by_id(page2.id()).unwrap();
    assert_eq!(found_page, page2);

    // Check pages iterator.
    let mut pages: Vec<_> = book
        .pages()
        .map(|page| page.expect("Invalid page"))
        .collect();

    pages.sort_by_key(|page| page.id());

    assert_eq!(book.num_pages(), 2);
    assert_eq!(pages[..], [page1, page2][..]);
}
