//! Reader for data blocks.

use core::num::NonZeroUsize;
use std::io::{self, Read, Seek, SeekFrom};

use super::BlockType;

/// Size of the LRU cache.
const LRU_CACHE_SIZE: NonZeroUsize = match NonZeroUsize::new(16) {
    // TODO use Option::unwrap when const_option feature is stable.
    Some(n) => n,
    None => panic!(),
};

pub(crate) struct DataBlocksReader<S> {
    stream: S,

    stream_len: u64,

    cache: lru::LruCache<u64, Result<Vec<u8>, io::Error>>,
}

impl<S: Read + Seek> DataBlocksReader<S> {
    pub(crate) fn new(mut stream: S) -> io::Result<Self> {
        let stream_len = stream.seek(SeekFrom::End(0))?;
        let cache = lru::LruCache::new(LRU_CACHE_SIZE);

        Ok(DataBlocksReader {
            stream,
            stream_len,
            cache,
        })
    }

    /// Return a mutable reference to the input stream.
    pub(crate) fn input_stream(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Return the known length of the stream.
    pub(crate) fn input_stream_len(&self) -> u64 {
        self.stream_len
    }

    /// Get a block from its identifier.
    ///
    /// The function is applied only if the block can be fully read, and the
    /// offset is within the block.
    pub(crate) fn with_block<F, T, O>(&mut self, block_id: u64, offset: O, f: F) -> io::Result<T>
    where
        F: FnOnce(&[u8]) -> T,
        O: TryInto<usize>,
    {
        let offset = O::try_into(offset).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "offset cannot be converted to usize",
            )
        })?;

        let result = self.cache.get_or_insert(block_id, || {
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

            if offset > len as usize {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "offset is beyond end of the block",
                ));
            }

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

            Ok(data)
        });

        result
            .as_ref()
            .map(|data| f(&data[offset..]))
            .map_err(|e| match e.get_ref() {
                Some(r) => io::Error::new(e.kind(), r.to_string()),
                None => io::Error::new(e.kind(), ""),
            })
    }
}
