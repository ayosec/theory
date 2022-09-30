//! List of key-value pairs.
//!
//! This module provide functions to serialize and deserialize a list of
//! key-value pairs.
//!
//! Keys are identified by a byte-tag, which must be associated with an `enum`
//! type. This `enum` type must implement `Into<u8>`.
//!
//! # Binary Format
//!
//! The binary format is stable, and it should be easy to read from other
//! programs.
//!
//! * The first byte is a tag to indicate the type.
//! * The next bytes is the length of the value, encoded as LEB128.
//! * The rest of the bytes is the value.
//!
//! A key with tag `0` is used to indicate that all pairs have been read.

use std::io::{self, Read, Write};
use std::marker::PhantomData;

pub(crate) enum InnerValue<'a> {
    Array8([u8; 8]),
    Slice(&'a [u8]),
}

impl AsRef<[u8]> for InnerValue<'_> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Array8(a) => &a[..],
            Self::Slice(s) => s,
        }
    }
}

/// Provide the functions to access the inner value of a variant, and to convert
/// a byte sequence to the original value.
pub(crate) trait VariantValue: Sized {
    /// Type associated to the byte-tag.
    type Key: Copy + Into<u8> + TryFrom<u8>;

    /// Errors from `deserialize`.
    type DeserializeError: std::fmt::Display;

    /// Returns the byte-tag and the representation, as a byte array, of the
    /// inner value.
    ///
    /// The byte-tag must not be `0`.
    fn serialize(&self) -> (Self::Key, InnerValue);

    /// Convert a byte sequence to the original variant.
    fn deserialize(key: Self::Key, bytes: Vec<u8>) -> Result<Self, Self::DeserializeError>;
}

/// Serialize a list of key-value pairs.
pub(crate) fn serialize<'a, I, V, W>(mut output: W, pairs: I) -> io::Result<()>
where
    I: IntoIterator<Item = &'a V>,
    V: VariantValue + 'a,
    W: Write,
{
    for pair in pairs.into_iter() {
        let (key, value) = pair.serialize();
        let value = value.as_ref();

        debug_assert!(key.into() != 0);

        output.write_all(&[key.into()])?;
        leb128::write::unsigned(&mut output, value.len() as u64)?;
        output.write_all(value)?;
    }

    output.write_all(&[0])?;

    Ok(())
}

/// Deserialize a list of key-value pairs from the `input`.
///
/// `input_len` must indicate the size of the input. It is used to validate the
/// length found before every entry.
pub(crate) fn deserialize<V, R>(
    input: R,
    input_len: u64,
) -> impl Iterator<Item = Result<V, DeserializeError<<V as VariantValue>::DeserializeError>>>
where
    V: VariantValue,
    R: Read,
{
    StreamParser {
        input,
        input_len,
        io_valid: true,
        phantom: PhantomData,
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum DeserializeError<T: std::fmt::Display> {
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    #[error("Failed to read a LEB128 integer: {0}.")]
    Leb128Error(#[from] leb128::read::Error),

    #[error("Invalid length: {0}.")]
    InvalidLength(u64),

    #[error("Invalid tag.")]
    InvalidByteTag,

    #[error("Parser error: {0}.")]
    ParserError(T),
}

struct StreamParser<I, T> {
    input: I,
    input_len: u64,
    io_valid: bool,
    phantom: PhantomData<T>,
}

impl<V: VariantValue, I: Read> Iterator for StreamParser<I, V> {
    type Item = Result<V, DeserializeError<<V as VariantValue>::DeserializeError>>;

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

        let key = run!(V::Key::try_from(byte_tag[0]).map_err(|_| DeserializeError::InvalidByteTag));

        let value_len = run!(leb128::read::unsigned(&mut self.input));
        if value_len > self.input_len {
            return Some(Err(DeserializeError::InvalidLength(value_len)));
        }

        let mut value_bytes = vec![0; value_len as usize];
        run!(self.input.read_exact(&mut value_bytes));

        let item = V::deserialize(key, value_bytes).map_err(DeserializeError::ParserError);

        Some(item)
    }
}

#[cfg(test)]
mod tests {
    use super::InnerValue;
    use std::io::Cursor;

    #[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive, Copy, Clone)]
    #[repr(u8)]
    enum ByteTag {
        A = 1,
        B = 2,
    }

    #[derive(Debug, PartialEq)]
    enum Entry {
        A(String),
        B(u64),
    }

    impl super::VariantValue for Entry {
        type Key = ByteTag;

        type DeserializeError = Box<dyn std::error::Error>;

        fn serialize(&self) -> (Self::Key, InnerValue) {
            match self {
                Self::A(a) => (ByteTag::A, InnerValue::Slice(a.as_bytes())),
                Self::B(b) => (ByteTag::B, InnerValue::Array8(b.to_be_bytes())),
            }
        }

        fn deserialize(key: Self::Key, bytes: Vec<u8>) -> Result<Self, Self::DeserializeError> {
            match key {
                ByteTag::A => Ok(Self::A(String::from_utf8(bytes)?)),
                ByteTag::B => Ok(Self::B(u64::from_be_bytes(bytes[..].try_into()?))),
            }
        }
    }

    #[test]
    fn write_read() {
        use Entry::{A, B};

        let mut bytes = Vec::new();

        super::serialize(Cursor::new(&mut bytes), [&A("abcd".into()), &B(1234)]).unwrap();

        let input_len = bytes.len() as u64;
        let mut iter = super::deserialize::<Entry, _>(Cursor::new(&mut bytes), input_len);
        assert_eq!(iter.next().unwrap().unwrap(), A("abcd".into()));
        assert_eq!(iter.next().unwrap().unwrap(), B(1234));
        assert!(iter.next().is_none());
    }
}
