//! This module provides the implementation to persist book to files.

use std::io::{self, Read, Seek, Write};

use crate::builder::BookBuilder;

mod v1;

pub(crate) mod datablock;
pub(crate) mod kvlist;

/// Errors related to persistence operations.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}.")]
    Io(#[from] io::Error),

    #[error("Integer number: {0}.")]
    InvalidNumber(#[from] std::num::TryFromIntError),

    #[error("Invalid magic number.")]
    InvalidMagic,

    #[error("The book contains too many pages.")]
    TooManyPages,

    #[error("Unable to load page index.")]
    PageError(#[from] crate::page::Error),
}

/// Expected size for magic numbers.
const MAGIC_SIZE: usize = 8;

/// Load a book from an input, like a file or a byte array.
///
/// The input is expected to be generated  by the [`dump`] function.
pub(crate) fn load<I>(mut input: I) -> Result<crate::Book<I>, Error>
where
    I: Read + Seek,
{
    let mut magic = [0; MAGIC_SIZE];
    input
        .read_exact(&mut magic)
        .map_err(|_| Error::InvalidMagic)?;

    match &magic {
        v1::MAGIC => v1::load(input),

        _ => Err(Error::InvalidMagic),
    }
}

/// Dump the content of the book in the output stream.
pub(crate) fn dump<O>(output: O, book: &BookBuilder) -> Result<(), Error>
where
    O: Write + Seek,
{
    v1::dump(output, book)
}
