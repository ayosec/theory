//! Reader for data blocks.

use std::io::{self, Read, Seek, SeekFrom};

use super::BlockType;

pub(crate) struct DataBlocksReader<S> {
    stream: S,
}

impl<S: Read + Seek> DataBlocksReader<S> {
    pub(crate) fn new(stream: S) -> Self {
        DataBlocksReader { stream }
    }

    /// Return a mutable reference to the input stream.
    pub(crate) fn input_stream(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Get a block from its identifier. The function is applied only if the
    /// block can be fully read.
    pub(crate) fn with_block<F, T>(&mut self, block_id: u64, f: F) -> io::Result<T>
    where
        F: FnOnce(&[u8]) -> T,
    {
        // TODO use LRU cache
        self.stream.seek(SeekFrom::Start(block_id))?;

        // Block type.
        let mut byte_tag = [0];
        self.stream.read_exact(&mut byte_tag)?;

        let block_type = BlockType::try_from(byte_tag[0])
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Invalid block type"))?;

        // Block length.
        let mut len = [0; 4];
        self.stream.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len);

        // Block data.
        let mut data;

        match block_type {
            BlockType::Uncompressed => {
                data = vec![0; len as usize];
                self.stream.read_exact(&mut data)?;
            }
        }

        Ok(f(&data[..]))
    }
}
