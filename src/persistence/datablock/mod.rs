//! _Data blocks_ are used to store any content in chunks. Data is referenced by
//! a block identifier and an offset inside it.
//!
//! # Block Format
//!
//! The first byte in the block indicates the compression, or `0` is the data is
//! not compressed.
//!
//! The next 4 bytes are the length of the block (`u32`, big-endian).

mod reader;
mod writer;

#[cfg(test)]
mod tests;

/// Tag to indicate the block type.
#[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive, Debug, Copy, Clone)]
#[repr(u8)]
enum BlockType {
    Uncompressed = 1,

    #[cfg(feature = "deflate")]
    Deflate = 2,
}

pub(crate) use reader::DataBlocksReader;
pub(crate) use writer::DataBlocksWriter;

/// Method to compress data in blocks.
#[derive(Default, Clone, Copy, Debug)]
pub enum BlockCompression {
    /// Don't compress data.
    #[default]
    None,

    /// Use DEFLATE, with the specified compression level (`0..=9`).
    #[cfg(feature = "deflate")]
    Deflate(u32),
}

impl BlockCompression {
    fn tag(&self) -> BlockType {
        match self {
            BlockCompression::None => BlockType::Uncompressed,

            #[cfg(feature = "deflate")]
            BlockCompression::Deflate(_) => BlockType::Deflate,
        }
    }
}
