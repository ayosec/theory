//! Support to include metadata entries in a book.

use std::io::{self, Read, Write};

use crate::persistence::kvlist;

/// Errors related to serialize operations.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid UTF-8 sequence.")]
    UnicodeError(#[from] std::string::FromUtf8Error),

    #[error("Invalid entry length.")]
    InvalidLength,
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
}

/// Metadata associated to a book.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum MetadataEntry {
    Title(String),
    Author(String),
    Language(String),
    Date(u64),
    License(String),
}

impl kvlist::VariantValue for MetadataEntry {
    type Key = ByteTag;
    type DeserializeError = Error;

    fn serialize(&self) -> (Self::Key, kvlist::InnerValue) {
        use kvlist::InnerValue::{Array8, Slice};

        let key = match self {
            MetadataEntry::Title(_) => ByteTag::Title,
            MetadataEntry::Author(_) => ByteTag::Author,
            MetadataEntry::Language(_) => ByteTag::Language,
            MetadataEntry::Date(_) => ByteTag::Date,
            MetadataEntry::License(_) => ByteTag::License,
        };

        let value = match self {
            MetadataEntry::Date(d) => Array8(d.to_be_bytes()),
            MetadataEntry::Title(s)
            | MetadataEntry::Author(s)
            | MetadataEntry::Language(s)
            | MetadataEntry::License(s) => Slice(s.as_bytes()),
        };

        (key, value)
    }

    fn deserialize(key: Self::Key, bytes: Vec<u8>) -> Result<Self, Self::DeserializeError> {
        macro_rules! value_str {
            ($variant:ident) => {
                String::from_utf8(bytes)
                    .map_err(|e| e.into())
                    .map(|s| MetadataEntry::$variant(s))
            };
        }

        match key {
            ByteTag::Title => value_str!(Title),
            ByteTag::Author => value_str!(Author),
            ByteTag::Language => value_str!(Language),
            ByteTag::License => value_str!(License),

            ByteTag::Date => bytes
                .try_into()
                .map(|b| MetadataEntry::Date(u64::from_be_bytes(b)))
                .map_err(|_| Error::InvalidLength),
        }
    }
}

/// Write metadata in the format described in the module documentation.
pub(crate) fn dump<'a, O, E, M>(output: O, metadata: M) -> io::Result<()>
where
    O: Write,
    E: Into<&'a MetadataEntry>,
    M: IntoIterator<Item = E>,
{
    kvlist::serialize(output, metadata.into_iter().map(|e| e.into()))
}

/// Return an iterator to get metadata entries from a `Read` stream.
pub(crate) fn load<I>(
    input: I,
) -> impl Iterator<Item = Result<MetadataEntry, kvlist::DeserializeError<Error>>>
where
    I: Read,
{
    kvlist::deserialize(input)
}
