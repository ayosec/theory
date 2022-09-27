//! Writer for data blocks.

use std::io::{self, Seek, SeekFrom, Write};
use std::mem;

use super::BlockType;

/// Size of the data block.
const MAX_DATA_BLOCK_SIZE: u64 = 32 * 1024;

/// Track the active block.
enum BlockState<S> {
    Invalid,

    Wait(S),

    Uncompressed {
        stream: S,
        block_id: u64,
        offset: u64,
    },
}

/// Data blocks generator.
pub(crate) struct DataBlocksWriter<S> {
    state: BlockState<S>,
}

impl<S: Write + Seek> DataBlocksWriter<S> {
    pub(crate) fn new(stream: S) -> Self {
        DataBlocksWriter {
            state: BlockState::Wait(stream),
        }
    }

    /// Closed the active block and move the writer to `Wait` state.
    fn close_current(&mut self) -> io::Result<()> {
        let (mut stream, len, block_id) = match mem::replace(&mut self.state, BlockState::Invalid) {
            BlockState::Wait(stream) => (stream, 0, 0),

            BlockState::Uncompressed {
                stream,
                offset,
                block_id,
            } => (stream, offset, block_id),

            BlockState::Invalid => unreachable!(),
        };

        // Write the block length (as u32, big-endian) after the tag.
        if len > 0 {
            let len_bytes = u32::try_from(len)
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::Other, "block size can't be written as u32")
                })?
                .to_be_bytes();

            let current = stream.stream_position()?;
            stream.seek(SeekFrom::Start(block_id + 1))?;
            stream.write_all(&len_bytes)?;
            stream.seek(SeekFrom::Start(current))?;
        }

        self.state = BlockState::Wait(stream);

        Ok(())
    }

    /// Creates a new fragment inside a data block.
    ///
    /// The fragment must be closed with its `finish()` function before creating
    /// another fragment.
    ///
    /// `size_hint` is used to determine if a new block should be created to
    /// store the data.
    pub(crate) fn fragment(&mut self, size_hint: u64) -> io::Result<Fragment<impl Write + Seek>> {
        let current_offset = match &self.state {
            BlockState::Uncompressed { offset, .. } => *offset,
            _ => 0,
        };

        if size_hint == u64::MAX
            || (current_offset + size_hint > MAX_DATA_BLOCK_SIZE && current_offset > 0)
        {
            self.close_current()?;
        }

        // Change to `Uncompressed` state if it is waiting.
        //
        // Every block starts with the byte-tag, and the length (u32).
        if let BlockState::Wait(_) = self.state {
            match mem::replace(&mut self.state, BlockState::Invalid) {
                BlockState::Wait(mut stream) => {
                    let block_id = stream.stream_position()?;

                    stream.write_all(&[BlockType::Uncompressed as u8, 0, 0, 0, 0])?;

                    self.state = BlockState::Uncompressed {
                        stream,
                        block_id,
                        offset: 0,
                    };
                }

                _ => unreachable!(),
            }
        }

        // Extract data from the state.
        match &mut self.state {
            BlockState::Uncompressed {
                stream,
                block_id,
                offset,
            } => {
                let offset_copy = *offset;
                let fragment = Fragment {
                    writer: stream,
                    writer_offset: offset,
                    block_id: *block_id,
                    offset: offset_copy,
                };

                Ok(fragment)
            }

            _ => unreachable!(),
        }
    }

    /// Close any active block, and return the underlying stream.
    pub(crate) fn finish(mut self) -> io::Result<S> {
        self.close_current()?;

        match self.state {
            BlockState::Wait(stream) => Ok(stream),
            _ => unreachable!(),
        }
    }
}

/// A fragment inside a data block. It is created with the
/// [`DataBlocksWriter::data`] function, and can be used to add
/// data to the data block.
pub(crate) struct Fragment<'a, S> {
    writer: &'a mut S,

    writer_offset: &'a mut u64,

    block_id: u64,

    offset: u64,
}

impl<S: Write + Seek> Fragment<'_, S> {
    /// Return the identifier of the data block that contains this fragment.
    pub(crate) fn block_id(&self) -> u64 {
        self.block_id
    }

    /// Return the offset inside the data block of the start of this fragment.
    pub(crate) fn offset(&self) -> u64 {
        self.offset
    }
}

impl<S: Write> Write for Fragment<'_, S> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        let n = self.writer.write(buf)?;
        *self.writer_offset += buf.len() as u64;
        Ok(n)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.writer.flush()
    }
}
