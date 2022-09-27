//! This module provide the implementation to create a new book.

use std::fs::File;
use std::io::{BufWriter, Seek, Write};
use std::num::NonZeroU32;
use std::path::Path;

use crate::{persistence, MetadataEntry, Page};

/// A builder for new books.
///
/// The pages are kept in memory, and then they can be stored with
/// [`BookBuilder::dump()`].
///
/// # Example
///
/// ```
/// use theory::{MetadataEntry, Book, Page};
/// use std::io::{Cursor, Read, Write};
///
/// let mut buffer: Vec<u8> = Vec::new();
///
/// let mut builder = Book::builder();
/// builder.new_page("First").set_content("1");
/// builder.new_page("Second").set_content("2");
///
/// builder
///     .add_metadata(MetadataEntry::Title("Theory Example".into()))
///     .dump(Cursor::new(&mut buffer));
///
/// let book = Book::load(Cursor::new(buffer)).unwrap();
///
/// assert_eq!(book.num_pages(), 2);
/// ```
pub struct BookBuilder {
    next_page_id: NonZeroU32,

    pub(crate) metadata: Vec<MetadataEntry>,

    pub(crate) pages: Vec<Page>,
}

impl BookBuilder {
    /// Creates a new instance.
    pub(crate) fn new() -> BookBuilder {
        BookBuilder {
            next_page_id: NonZeroU32::new(1).unwrap(),
            metadata: Vec::new(),
            pages: Vec::new(),
        }
    }

    /// Add a new metadata entry.
    ///
    /// The same metadata entry type can appear multiple times, but the reader
    /// can choose to show only one of them.
    pub fn add_metadata(&mut self, entry: MetadataEntry) -> &mut BookBuilder {
        self.metadata.push(entry);
        self
    }

    /// Create a new page with a title. The content of the page is set using the
    /// mutable reference returned by this function.
    ///
    /// Its content can be set with the [`set_keywords`], [`description`], and
    /// [`set_content`] functions.
    ///
    /// [`set_keywords`]: Page::set_keywords
    /// [`description`]: Page::set_description
    /// [`set_content`]: Page::set_content
    pub fn new_page(&mut self, title: impl Into<String>) -> &mut Page {
        let page = Page::new(title.into(), self.next_page_id);
        self.next_page_id = self.next_page_id.saturating_add(1);
        self.pages.push(page);
        self.pages.last_mut().unwrap()
    }

    /// Dump this book to the specified stream. The written data can be
    /// loaded with [`load`](crate::Book::load).
    pub fn dump<O>(&self, output: O) -> Result<(), persistence::Error>
    where
        O: Write + Seek,
    {
        persistence::dump(output, self)
    }

    /// Dump this page to the specified file.
    ///
    /// See [`dump`](Self::dump) for more details.
    pub fn dump_to_file(&self, path: impl AsRef<Path>) -> Result<(), persistence::Error> {
        persistence::dump(BufWriter::new(File::create(path)?), self)
    }
}
