//! Module with the `Book` implementation.

use std::io::{self, Read, Seek, SeekFrom};

use crate::builder::BookBuilder;
use crate::errors::MetadataError;
use crate::persistence::datablock::DataBlocksReader;
use crate::{metadata, page, persistence, toc, MetadataEntry};

/// A book loaded from an input stream, like a file.
pub struct Book<I> {
    /// Data blocks in the input stream.
    pub(crate) data_blocks: DataBlocksReader<I>,

    /// Number of pages in the book.
    pub(crate) num_pages: usize,

    /// Position, in bytes, of the metadata table in the input.
    pub(crate) metadata_pos: usize,

    /// Page index loaded from the input.
    pub(crate) page_index: page::Index,
}

impl Book<()> {
    /// Return an instance of [`BookBuilder`], which can be used to build a
    /// new book, with its own pages and metadata.
    pub fn builder() -> BookBuilder {
        BookBuilder::new()
    }
}

impl<I: Read + Seek> Book<I> {
    /// Load book from a stream, serialized with [`BookBuilder::dump()`].
    pub fn load(input: I) -> Result<Self, persistence::PersistenceError> {
        persistence::load(input)
    }

    /// Return the number of pages included in the book.
    pub fn num_pages(&self) -> usize {
        self.num_pages
    }

    /// Return an iterator to get all metadata entries in the book.
    pub fn metadata(
        &mut self,
    ) -> io::Result<impl Iterator<Item = Result<MetadataEntry, MetadataError>> + '_> {
        let input_len = self.data_blocks.input_stream_len();
        let input = self.data_blocks.input_stream();
        input.seek(SeekFrom::Start(self.metadata_pos as u64))?;
        Ok(metadata::load(input, input_len))
    }

    /// Return an iterator to get all pages in the book.
    pub fn pages(&mut self) -> impl Iterator<Item = Result<page::Page, page::PageError>> + '_ {
        self.page_index.pages_iter(&mut self.data_blocks)
    }

    /// Return a single page by its identifier.
    pub fn get_page_by_id(&mut self, page_id: page::PageId) -> Result<page::Page, page::PageError> {
        self.page_index.get_by_id(&mut self.data_blocks, page_id)
    }

    /// Table of contents of this book.
    pub fn toc(&mut self) -> Result<impl Iterator<Item = crate::TocEntry> + '_, toc::TocError> {
        let toc = toc::BookToc::new(&mut self.data_blocks, &self.page_index)?;
        Ok(toc.into_iter())
    }
}
