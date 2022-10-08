//! Types for the metadata functionality.
//!
//! Both `Page` and `Book` support any number of metadata entries, which are
//! defined as [`MetadataEntry`] values.
//!
//! # Binary Format
//!
//! Metadata entries are encoded as a list of key-value pairs.
//!
//! The binary format is stable, and it should be easy to read from other
//! programs.
//!
//! * The first byte is a tag to indicate the type.
//! * The next bytes is the length of the value, encoded as LEB128.
//! * The rest of the bytes is the value.
//!
//! If the entry has multiple values (like `MetadataEntry::User`), each value is
//! preceded by its length in bytes.
//!
//! Tag `0` is used to indicate that all pairs have been read.

use std::io::{self, Read, Write};

/// Errors related to serialize operations.
#[derive(thiserror::Error, Debug)]
pub enum MetadataError {
    /// The byte sequence did not contain valid Unicode data.
    #[error("Invalid UTF-8 sequence.")]
    UnicodeError(#[from] std::string::FromUtf8Error),

    /// Failed to get data from the input.
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Failed to decode a LEB128-encoded number.
    #[error("Failed to read a LEB128 integer: {0}.")]
    Leb128Error(#[from] leb128::read::Error),

    /// A length value read from the input is not valid.
    #[error("Invalid length: {0}.")]
    InvalidLength(u64),

    /// Unknown byte tag for a metadata entry.
    #[error("Invalid tag.")]
    InvalidByteTag(u8),
}

/// A number to specify the type of the entry in the metadata table.
///
/// To keep backwards compatibility, new entry types must not reuse numbers of
/// previous ones.
#[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive, Debug, Copy, Clone)]
#[repr(u8)]
pub(crate) enum ByteTag {
    Title = 1,
    Author = 2,
    Language = 3,
    Date = 4,
    License = 5,
    Keyword = 6,
    User = 100,
}

/// Metadata associated to a [book](crate::Book) or a [page](crate::Page).
///
/// # Adding Metadata Entries
///
/// Entries can be added with the `add_metadata` function of each type.
///
/// ```rust
/// use theory::{Book, MetadataEntry::{Keyword, Title}};
///
/// let mut builder = Book::builder();
/// builder.add_metadata(Title("The Book".into()));
///
/// builder
///     .new_page("Introduction")
///     .add_metadata(Keyword("intro".into()));
/// ```
///
/// # Reading Metadata Entries
///
/// Both [`Book`](crate::Book) and [`Page`](crate::Page) provide a `metadata`
/// function to get all entries associated with the item.
///
/// For example, to get the title of a book:
///
/// ```
/// use std::io::{Read, Seek};
/// use theory::{Book, MetadataEntry::Title, errors::MetadataError};
///
/// fn book_title<T>(book: &mut Book<T>) -> Result<Option<String>, MetadataError>
/// where
///     T: Read + Seek,
/// {
///     for entry in book.metadata()? {
///         if let Title(t) = entry? {
///             return Ok(Some(t));
///         }
///     }
///
///     Ok(None)
/// }
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum MetadataEntry {
    Title(String),
    Author(String),
    Language(String),
    Date(u64),
    License(String),
    Keyword(String),
    User(String, String),
}

/// Write metadata in the format described in the module documentation.
pub(crate) fn dump<'a, O, M>(mut output: O, metadata: M) -> io::Result<()>
where
    O: Write,
    M: IntoIterator<Item = &'a MetadataEntry>,
{
    for entry in metadata.into_iter() {
        macro_rules! w {
            ($tag:ident, $($values:expr),*) => {{
                let tag: u8 = ByteTag::$tag.into();
                debug_assert!(tag != 0);

                output.write_all(&[tag])?;

                $(
                    let bytes = $values;

                    leb128::write::unsigned(&mut output, bytes.len() as u64)?;
                    output.write_all(bytes)?;
                )*
            }}
        }

        match entry {
            MetadataEntry::Title(s) => w!(Title, s.as_bytes()),
            MetadataEntry::Author(s) => w!(Author, s.as_bytes()),
            MetadataEntry::Language(s) => w!(Language, s.as_bytes()),
            MetadataEntry::Date(d) => w!(Date, &d.to_be_bytes()),
            MetadataEntry::License(s) => w!(License, s.as_bytes()),
            MetadataEntry::Keyword(s) => w!(Keyword, s.as_bytes()),
            MetadataEntry::User(k, v) => w!(User, k.as_bytes(), v.as_bytes()),
        }
    }

    output.write_all(&[0])?;

    Ok(())
}

/// Return an iterator to get metadata entries from a `Read` stream.
pub(crate) fn load<I>(
    input: I,
    input_len: u64,
) -> impl Iterator<Item = Result<MetadataEntry, MetadataError>>
where
    I: Read,
{
    BinaryDataParser {
        input,
        input_len,
        io_valid: true,
    }
}

struct BinaryDataParser<I> {
    input: I,
    input_len: u64,
    io_valid: bool,
}

impl<I: Read> Iterator for BinaryDataParser<I> {
    type Item = Result<MetadataEntry, MetadataError>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.io_valid {
            return None;
        }

        macro_rules! run {
            ($io:expr) => {
                match $io {
                    Ok(result) => result,
                    Err(e) => {
                        self.io_valid = false;
                        return Some(Err(e.into()));
                    }
                }
            };
        }

        let mut byte_tag = [0xFF];
        run!(self.input.read_exact(&mut byte_tag));

        if byte_tag[0] == 0 {
            return None;
        }

        let key =
            run!(ByteTag::try_from(byte_tag[0])
                .map_err(|_| MetadataError::InvalidByteTag(byte_tag[0])));

        macro_rules! next_value {
            () => {{
                let value_len = run!(leb128::read::unsigned(&mut self.input));
                if value_len > self.input_len {
                    self.io_valid = false;
                    return Some(Err(MetadataError::InvalidLength(value_len)));
                }

                let mut value_bytes = vec![0; value_len as usize];
                run!(self.input.read_exact(&mut value_bytes));

                value_bytes
            }};
        }

        macro_rules! next_str {
            () => {
                run!(String::from_utf8(next_value!()).map_err(MetadataError::UnicodeError))
            };
        }

        let item = match key {
            ByteTag::Title => Ok(MetadataEntry::Title(next_str!())),
            ByteTag::Author => Ok(MetadataEntry::Author(next_str!())),
            ByteTag::Language => Ok(MetadataEntry::Language(next_str!())),
            ByteTag::License => Ok(MetadataEntry::License(next_str!())),
            ByteTag::Keyword => Ok(MetadataEntry::Keyword(next_str!())),
            ByteTag::User => Ok(MetadataEntry::User(next_str!(), next_str!())),

            ByteTag::Date => next_value!()
                .try_into()
                .map(|b| MetadataEntry::Date(u64::from_be_bytes(b)))
                .map_err(|e| MetadataError::InvalidLength(e.len() as u64)),
        };

        Some(item)
    }
}

#[test]
fn write_read_metadata() {
    let entries = [
        MetadataEntry::Title("title".into()),
        MetadataEntry::Date(1234567890),
        MetadataEntry::User("key".into(), "value".into()),
    ];

    let mut buf = Vec::new();
    dump(io::Cursor::new(&mut buf), &entries).unwrap();

    let loaded = load(io::Cursor::new(&buf), buf.len() as u64)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(entries, loaded[..]);
}
