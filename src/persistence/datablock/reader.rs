//! Reader for data blocks.

use std::io::{self, Read, Seek, SeekFrom};

use super::BlockType;

pub(crate) struct DataBlocksReader<S> {
    stream: S,

    stream_len: u64,
}

impl<S: Read + Seek> DataBlocksReader<S> {
    pub(crate) fn new(mut stream: S) -> io::Result<Self> {
        let stream_len = stream.seek(SeekFrom::End(0))?;
        Ok(DataBlocksReader { stream, stream_len })
    }

    /// Return a mutable reference to the input stream.
    pub(crate) fn input_stream(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Return the known length of the stream.
    pub(crate) fn input_stream_len(&self) -> u64 {
        self.stream_len
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
        //
        // Return an error if the length is beyond the end of the input.
        let mut len = [0; 4];
        self.stream.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len);

        if block_id.saturating_add(len as u64) > self.stream_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Block beyond the end of the input",
            ));
        }

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
