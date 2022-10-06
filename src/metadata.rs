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
    Keyword = 6,
    User = 100,
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
    Keyword(String),
    User(String, String),
}

impl kvlist::VariantValue for MetadataEntry {
    type Key = ByteTag;
    type DeserializeError = Error;

    fn serialize(&self) -> (Self::Key, kvlist::InnerValue) {
        use kvlist::InnerValue::{Array8, Buffer, Slice};

        let key = match self {
            MetadataEntry::Title(_) => ByteTag::Title,
            MetadataEntry::Author(_) => ByteTag::Author,
            MetadataEntry::Language(_) => ByteTag::Language,
            MetadataEntry::Date(_) => ByteTag::Date,
            MetadataEntry::License(_) => ByteTag::License,
            MetadataEntry::Keyword(_) => ByteTag::Keyword,
            MetadataEntry::User(_, _) => ByteTag::User,
        };

        let value = match self {
            MetadataEntry::Date(d) => Array8(d.to_be_bytes()),

            MetadataEntry::Title(s)
            | MetadataEntry::Author(s)
            | MetadataEntry::Language(s)
            | MetadataEntry::License(s)
            | MetadataEntry::Keyword(s) => Slice(s.as_bytes()),

            MetadataEntry::User(k, v) => {
                let mut buffer = Vec::with_capacity(k.len() + v.len() + 1);
                leb128::write::unsigned(&mut buffer, k.len() as u64)
                    .unwrap_or_else(|_| unreachable!());
                buffer.extend_from_slice(k.as_bytes());
                buffer.extend_from_slice(v.as_bytes());
                Buffer(buffer)
            }
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
            ByteTag::Keyword => value_str!(Keyword),

            ByteTag::Date => bytes
                .try_into()
                .map(|b| MetadataEntry::Date(u64::from_be_bytes(b)))
                .map_err(|_| Error::InvalidLength),

            ByteTag::User => {
                let bytes_len = bytes.len() as u64;
                let mut input = io::Cursor::new(bytes);

                let value_len =
                    leb128::read::unsigned(&mut input).map_err(|_| Error::InvalidLength)?;

                if value_len > bytes_len {
                    return Err(Error::InvalidLength);
                }

                let mut key = vec![0; value_len as usize];
                input
                    .read_exact(&mut key)
                    .map_err(|_| Error::InvalidLength)?;

                let key = String::from_utf8(key).map_err(Error::UnicodeError)?;

                let mut value = Vec::with_capacity((bytes_len - input.position()) as usize);
                input
                    .read_to_end(&mut value)
                    .map_err(|_| Error::InvalidLength)?;

                let value = String::from_utf8(value).map_err(Error::UnicodeError)?;

                Ok(MetadataEntry::User(key, value))
            }
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
    input_len: u64,
) -> impl Iterator<Item = Result<MetadataEntry, kvlist::DeserializeError<Error>>>
where
    I: Read,
{
    kvlist::deserialize(input, input_len)
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
