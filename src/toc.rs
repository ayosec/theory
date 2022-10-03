//! This module provides types to read the TOC of a book.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek};

use crate::page::{Index, PageId};
use crate::persistence::datablock::DataBlocksReader;

/// Maximum subsections level when compute the TOC.
///
/// Its main use is to avoid cyclic references.
const MAX_SUB_LEVEL: usize = 32;

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
    LevelLoop,
}

/// Entry in the TOC tree.
#[derive(Debug)]
pub struct TocEntry {
    /// Page identifier.
    id: PageId,

    /// Title of the page.
    title: String,

    /// A list to describe the section number.
    section_number: SectionNumbers,

    /// Pages under this level.
    children: BTreeMap<PageId, TocEntry>,
}

impl TocEntry {
    fn new(id: PageId, title: String, section_number: SectionNumbers) -> TocEntry {
        TocEntry {
            id,
            title,
            section_number,
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
    pub fn section_number(&self) -> &[u16] {
        self.section_number.as_ref()
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
                    let section = tree.len() as u16 + 1;
                    tree.insert(
                        *id,
                        TocEntry::new(*id, title, SectionNumbers::from([section])),
                    );
                }

                Some(parent_id) => {
                    // Compute the path using the cache in `parents`.
                    let mut path = [None; MAX_SUB_LEVEL];
                    let mut last_id = parent_id;

                    for slot in &mut path {
                        *slot = Some(last_id);

                        match parents.get(&last_id) {
                            Some(None) => break,
                            Some(Some(next_id)) => last_id = *next_id,
                            None => return Err(TocError::InvalidParent(parent_id)),
                        }
                    }

                    // If the last item is filled, we don't know if the
                    // hierarchy is completed, so we assume that the levels
                    // are too deep.
                    if path.last().map(|l| l.is_some()) == Some(true) {
                        return Err(TocError::LevelLoop);
                    }

                    let target = path
                        .into_iter()
                        .rev()
                        .skip_while(|item| item.is_none())
                        .flatten()
                        .try_fold((None, &mut tree), |(_, tree), id| {
                            tree.get_mut(&id)
                                .map(|t| (Some(&t.section_number), &mut t.children))
                        });

                    match target {
                        Some((Some(section_number), target)) => {
                            let mut section_number = section_number.clone();
                            section_number.push(target.len() as u16 + 1);
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

/// Container for the section numbers
#[derive(Debug, Clone)]
enum SectionNumbers {
    Few { len: u8, numbers: [u16; 4] },

    Many(Vec<u16>),
}

impl SectionNumbers {
    fn new() -> Self {
        SectionNumbers::Few {
            len: 0,
            numbers: [0; 4],
        }
    }

    fn push(&mut self, number: u16) {
        match self {
            SectionNumbers::Few { len, numbers } => {
                let pos = *len as usize;
                if pos < numbers.len() {
                    numbers[pos] = number;
                    *len += 1;
                } else {
                    let mut vec = Vec::from(&numbers[..]);
                    vec.push(number);
                    *self = SectionNumbers::Many(vec);
                }
            }

            SectionNumbers::Many(vec) => {
                vec.push(number);
            }
        }
    }
}

impl<T: IntoIterator<Item = u16>> From<T> for SectionNumbers {
    fn from(numbers: T) -> SectionNumbers {
        let mut sn = SectionNumbers::new();
        for number in numbers {
            sn.push(number);
        }
        sn
    }
}

impl AsRef<[u16]> for SectionNumbers {
    fn as_ref(&self) -> &[u16] {
        match self {
            SectionNumbers::Few { len, numbers } => &numbers[..*len as usize],
            SectionNumbers::Many(vec) => &vec[..],
        }
    }
}

#[cfg(test)]
mod tests {

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
        let mut toc = book.toc().expect("Book::toc").into_iter();

        macro_rules! assert_page {
            ($iter:expr, $id:expr, $title:expr, $section:expr) => {
                match $iter.next().expect("Iterator must return something") {
                    item => {
                        assert_eq!(item.id(), $id);
                        assert_eq!(item.title(), $title);
                        assert_eq!(item.section_number(), &$section);
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
    fn section_numbers() {
        for max in 0..20 {
            let mut sn = super::SectionNumbers::new();
            let mut expected = vec![];

            for n in 1..=max {
                sn.push(n);
                expected.push(n);
            }

            assert_eq!(sn.as_ref(), &expected);
        }
    }
}
