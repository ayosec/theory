//! This module provides types to read the TOC of a book.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek};

use crate::page::{Index, PageId};
use crate::persistence::datablock::DataBlocksReader;

use tinyvec::{ArrayVec, TinyVec};

/// Maximum subsections level when compute the TOC.
///
/// Its main use is to avoid cyclic references.
const MAX_SUB_LEVEL: usize = 32;

/// List of section numbers to reference a page.
type SectionNumbers = TinyVec<[u32; 4]>;

/// Error raised when accessing the table of contents.
#[derive(thiserror::Error, Debug)]
pub enum TocError {
    #[error("Invalid parent identifier: {0:?}.")]
    InvalidParent(PageId),

    #[error("I/O error: {0}.")]
    Io(#[from] std::io::Error),

    #[error("Failed to get title: {0}.")]
    TitleError(crate::page::Error),

    #[error("Too many nested levels.")]
    ParentLoop,
}

/// Entry in the TOC tree.
#[derive(Debug)]
pub struct TocEntry {
    /// Page identifier.
    id: PageId,

    /// Title of the page.
    title: String,

    /// A list to describe the section number.
    section_numbers: SectionNumbers,

    /// Pages under this level.
    children: BTreeMap<PageId, TocEntry>,
}

impl TocEntry {
    fn new(id: PageId, title: String, section_numbers: SectionNumbers) -> TocEntry {
        TocEntry {
            id,
            title,
            section_numbers,
            children: BTreeMap::new(),
        }
    }

    /// Page identifier.
    pub fn id(&self) -> PageId {
        self.id
    }

    /// Title of the page.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Section number of the page.
    ///
    /// The list includes the section numbers of the parents.
    pub fn section_numbers(&self) -> &[u32] {
        self.section_numbers.as_ref()
    }

    /// List of pages under this one.
    pub fn children(&self) -> impl Iterator<Item = &'_ TocEntry> {
        self.children.values()
    }
}

/// Table of contents of a book.
pub struct BookToc {
    tree: BTreeMap<PageId, TocEntry>,
}

impl BookToc {
    pub(crate) fn new<I>(
        data_blocks: &mut DataBlocksReader<I>,
        index: &Index,
    ) -> Result<Self, TocError>
    where
        I: Read + Seek,
    {
        let mut parents = HashMap::new();
        let mut tree = BTreeMap::new();

        for (id, index_entry) in index {
            let parent_id = index_entry.parent_id();

            parents.insert(*id, parent_id);

            let title = index_entry
                .get_page_title(data_blocks)
                .map_err(TocError::TitleError)?;

            match parent_id {
                None => {
                    let section = tree.len() as u32 + 1;
                    tree.insert(
                        *id,
                        TocEntry::new(*id, title, SectionNumbers::from(&[section][..])),
                    );
                }

                Some(parent_id) => {
                    // Compute the path using the cache in `parents`.
                    let mut path = ArrayVec::<[_; MAX_SUB_LEVEL]>::new();
                    let mut last_id = parent_id;

                    loop {
                        if path.try_push(Some(last_id)).is_some() {
                            return Err(TocError::ParentLoop);
                        }

                        match parents.get(&last_id) {
                            Some(None) => break,
                            Some(Some(next_id)) => last_id = *next_id,
                            None => return Err(TocError::InvalidParent(parent_id)),
                        }
                    }

                    let target = path.into_iter().flatten().rev().try_fold(
                        (None, &mut tree),
                        |(_, tree), id| {
                            tree.get_mut(&id)
                                .map(|t| (Some(&t.section_numbers), &mut t.children))
                        },
                    );

                    match target {
                        Some((Some(section_number), target)) => {
                            let mut section_number = section_number.clone();
                            section_number.push(target.len() as u32 + 1);
                            target.insert(*id, TocEntry::new(*id, title, section_number));
                        }

                        _ => return Err(TocError::InvalidParent(parent_id)),
                    }
                }
            }
        }

        Ok(BookToc { tree })
    }
}

impl IntoIterator for BookToc {
    type Item = TocEntry;
    type IntoIter = std::collections::btree_map::IntoValues<PageId, TocEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.tree.into_values()
    }
}

#[cfg(test)]
mod tests {
    use crate::page::PageId;
    use crate::Book;
    use std::io::Cursor;

    #[test]
    fn compute_toc() {
        let mut builder = Book::builder();

        let p1 = builder.new_page("A").id();
        let p2 = builder.new_page("B").id();
        let p1_1 = builder.new_page("C").set_parent(p1).id();
        let p1_2 = builder.new_page("D").set_parent(p1).id();
        let p1_3 = builder.new_page("E").set_parent(p1).id();
        let p2_1 = builder.new_page("F").set_parent(p2).id();
        let p2_1_1 = builder.new_page("G").set_parent(p2_1).id();

        let mut buffer: Vec<u8> = Vec::new();
        builder
            .dump(Cursor::new(&mut buffer))
            .expect("BookBuilder::dump");

        let mut book = Book::load(Cursor::new(buffer)).unwrap();
        let mut toc = book.toc().expect("Book::toc");

        macro_rules! assert_page {
            ($iter:expr, $id:expr, $title:expr, $section:expr) => {
                match $iter.next().expect("Iterator must return something") {
                    item => {
                        assert_eq!(item.id(), $id);
                        assert_eq!(item.title(), $title);
                        assert_eq!(item.section_numbers(), &$section);
                        item
                    }
                }
            };
        }

        let entry = assert_page!(toc, p1, "A", [1]);
        let mut children = entry.children();

        assert_page!(children, p1_1, "C", [1, 1]);
        assert_page!(children, p1_2, "D", [1, 2]);
        assert_page!(children, p1_3, "E", [1, 3]);

        let entry = assert_page!(toc, p2, "B", [2]);
        let entry = assert_page!(entry.children(), p2_1, "F", [2, 1]);
        assert_page!(entry.children(), p2_1_1, "G", [2, 1, 1]);
    }

    #[test]
    fn detect_loops() {
        let mut buffer: Vec<u8> = Vec::new();
        let mut builder = Book::builder();

        builder.new_page("A").set_parent(PageId::force_value(1));

        builder
            .dump(Cursor::new(&mut buffer))
            .expect("BookBuilder::dump");

        let mut book = Book::load(Cursor::new(buffer)).unwrap();
        assert!(matches!(book.toc(), Err(super::TocError::ParentLoop)));
    }
}
