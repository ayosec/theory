//! This module provides the [`Page`] type, which contains a single page in
//! memory.

pub(crate) mod persistence;

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroU32;

use self::persistence::IndexEntry;
use crate::persistence::datablock::DataBlocksReader;
use crate::MetadataEntry;

use endiannezz::Io;

/// Errors related to page serialization.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}.")]
    Io(#[from] std::io::Error),

    #[error("Invalid UTF-8 sequence.")]
    UnicodeError(#[from] std::string::FromUtf8Error),

    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    #[error("Failed to read a LEB128 integer: {0}.")]
    Leb128Error(#[from] leb128::read::Error),

    #[error("Invalid length: {0}.")]
    InvalidLength(u64),

    #[error("Invalid page identifier: {0}")]
    InvalidId(u32),

    #[error("Duplicated page identifier ({0})")]
    DuplicatedId(u32),
}

/// Page identifier.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord)]
pub struct PageId(NonZeroU32);

impl From<PageId> for u32 {
    fn from(id: PageId) -> u32 {
        id.0.get()
    }
}

impl PageId {
    #[cfg(test)]
    pub(crate) fn force_value(id: u32) -> PageId {
        PageId(NonZeroU32::new(id).unwrap())
    }
}

/// A single page in a book.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Page {
    pub(crate) id: NonZeroU32,

    pub(crate) parent_id: Option<NonZeroU32>,

    pub(crate) metadata: Vec<MetadataEntry>,

    pub(crate) content: Vec<u8>,
}

impl Page {
    pub(crate) fn new(title: String, id: NonZeroU32) -> Page {
        Page {
            id,
            parent_id: None,
            metadata: vec![MetadataEntry::Title(title)],
            content: Vec::new(),
        }
    }

    /// Return the page identifier.
    pub fn id(&self) -> PageId {
        PageId(self.id)
    }

    /// Set the parent page.
    pub fn set_parent(&mut self, page_id: PageId) -> &mut Page {
        self.parent_id = Some(page_id.0);
        self
    }

    /// Add a metadata entry to the page.
    pub fn add_metadata(&mut self, entry: MetadataEntry) -> &mut Page {
        self.metadata.push(entry);
        self
    }

    /// Set the content for this page.
    pub fn set_content(&mut self, content: impl Into<Vec<u8>>) -> &mut Page {
        self.content = content.into();
        self
    }

    /// Return the parent of this page.
    pub fn parent(&self) -> Option<PageId> {
        self.parent_id.map(PageId)
    }

    /// Return the content of this page.
    pub fn content(&self) -> &[u8] {
        &self.content
    }

    /// Return the metadata of this page.
    pub fn metadata(&self) -> &[MetadataEntry] {
        &self.metadata
    }
}

/// Page index stored in the `page_pos` position.
pub(crate) struct Index {
    entries: BTreeMap<PageId, IndexEntry>,
}

impl Index {
    /// Load the page entries located at `position`.
    pub(crate) fn new<R>(mut input: R, num_pages: usize, position: u64) -> Result<Self, Error>
    where
        R: Read + Seek,
    {
        let mut entries = BTreeMap::new();
        input.seek(SeekFrom::Start(position))?;

        for _ in 0..num_pages {
            let ie = IndexEntry::read(&mut input)?;

            let page_id = match NonZeroU32::new(ie.id) {
                Some(id) => id,
                None => return Err(Error::InvalidId(ie.id)),
            };

            if entries.insert(PageId(page_id), ie).is_some() {
                return Err(Error::DuplicatedId(page_id.get()));
            }
        }

        Ok(Index { entries })
    }

    /// Get an iterator to get all pages in the book.
    pub(crate) fn pages_iter<'a, R>(
        &'a self,
        db_reader: &'a mut DataBlocksReader<R>,
    ) -> impl Iterator<Item = Result<Page, Error>> + 'a
    where
        R: Read + Seek + 'a,
    {
        self.entries
            .values()
            .map(move |entry| persistence::build_page(entry, db_reader))
    }

    /// Get a single page.
    pub(crate) fn get_by_id<R>(
        &mut self,
        db_reader: &mut DataBlocksReader<R>,
        page_id: PageId,
    ) -> Result<Page, Error>
    where
        R: Read + Seek,
    {
        let entry = match self.entries.get(&page_id) {
            Some(e) => e,
            None => return Err(Error::InvalidId(page_id.0.get())),
        };

        persistence::build_page(entry, db_reader)
    }
}

impl<'a> IntoIterator for &'a Index {
    type Item = (&'a PageId, &'a IndexEntry);
    type IntoIter = std::collections::btree_map::Iter<'a, PageId, IndexEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}
