//! This module provides the [`Page`] type, which contains a single page in
//! memory.

pub(crate) mod persistence;

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroU32;

use self::persistence::IndexEntry;
use crate::persistence::datablock::DataBlocksReader;

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

    #[error("Invalid input length: {0}.")]
    InvalidLength(#[from] leb128::read::Error),

    #[error("Invalid page identifier: {0}")]
    InvalidId(u32),

    #[error("Duplicated page identifier ({0})")]
    DuplicatedId(u32),
}

/// Page identifier.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord)]
pub struct PageId(NonZeroU32);

/// A single page in a book.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Page {
    pub(crate) id: NonZeroU32,

    pub(crate) parent_id: Option<NonZeroU32>,

    pub(crate) title: String,

    pub(crate) keywords: Option<String>,

    pub(crate) description: Option<String>,

    pub(crate) content: Option<String>,
}

impl Page {
    pub(crate) fn new(title: String, id: NonZeroU32) -> Page {
        Page {
            id,
            parent_id: None,
            title,
            keywords: None,
            description: None,
            content: None,
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

    /// Set the keywords for this page.
    pub fn set_keywords(&mut self, keywords: impl Into<String>) -> &mut Page {
        self.keywords = Some(keywords.into());
        self
    }

    /// Set the description for this page.
    pub fn set_description(&mut self, description: impl Into<String>) -> &mut Page {
        self.description = Some(description.into());
        self
    }

    /// Set the content for this page.
    pub fn set_content(&mut self, content: impl Into<String>) -> &mut Page {
        self.content = Some(content.into());
        self
    }
}

/// Page index stored in the `page_pos` position.
pub(crate) struct Index {
    map: HashMap<PageId, IndexEntry>,
}

impl Index {
    /// Load the page entries located at `position`.
    pub(crate) fn new<R>(mut input: R, num_pages: usize, position: u64) -> Result<Self, Error>
    where
        R: Read + Seek,
    {
        let mut map = HashMap::with_capacity(num_pages);
        input.seek(SeekFrom::Start(position))?;

        for _ in 0..num_pages {
            let ie = IndexEntry::read(&mut input)?;

            let page_id = match NonZeroU32::new(ie.id) {
                Some(id) => id,
                None => return Err(Error::InvalidId(ie.id)),
            };

            if map.insert(PageId(page_id), ie).is_some() {
                return Err(Error::DuplicatedId(page_id.get()));
            }
        }

        Ok(Index { map })
    }

    /// Get an iterator to get all pages in the book.
    pub(crate) fn pages_iter<'a, R>(
        &'a self,
        db_reader: &'a mut DataBlocksReader<R>,
    ) -> impl Iterator<Item = Result<Page, Error>> + 'a
    where
        R: Read + Seek + 'a,
    {
        self.map
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
        let entry = match self.map.get(&page_id) {
            Some(e) => e,
            None => return Err(Error::InvalidId(page_id.0.get())),
        };

        persistence::build_page(entry, db_reader)
    }
}
