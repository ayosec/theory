//! Writer for data blocks.

use std::io::{self, Seek, SeekFrom, Write};
use std::mem;

use super::BlockCompression;

#[cfg(feature = "deflate")]
use flate2::write::DeflateEncoder;

/// Size of the data block.
const MAX_DATA_BLOCK_SIZE: u64 = 64 * 1024;

/// Target of data block data.
enum Writer<S: Write> {
    Raw(S),

    #[cfg(feature = "deflate")]
    Deflate(DeflateEncoder<S>),

    #[cfg(feature = "lz4")]
    Lz4(lz4_flex::frame::FrameEncoder<S>),
}

impl<W: Write> Writer<W> {
    fn into_stream(self) -> io::Result<W> {
        match self {
            Writer::Raw(r) => Ok(r),

            #[cfg(feature = "deflate")]
            Writer::Deflate(d) => d.finish(),

            #[cfg(feature = "lz4")]
            Writer::Lz4(l) => l.finish().map_err(super::map_lz4_err),
        }
    }

    fn get_stream(&mut self) -> &mut dyn Write {
        match self {
            Writer::Raw(r) => r,

            #[cfg(feature = "deflate")]
            Writer::Deflate(d) => d,

            #[cfg(feature = "lz4")]
            Writer::Lz4(l) => l,
        }
    }
}

/// Track the active block.
enum BlockState<S: Write> {
    Invalid,

    Wait(S),

    Active {
        writer: Writer<S>,
        block_id: u64,
        offset: u64,
    },
}

/// Data blocks generator.
pub(crate) struct DataBlocksWriter<S: Write> {
    state: BlockState<S>,

    compression: BlockCompression,
}

impl<S: Write + Seek> DataBlocksWriter<S> {
    pub(crate) fn new(stream: S, compression: BlockCompression) -> Self {
        DataBlocksWriter {
            state: BlockState::Wait(stream),
            compression,
        }
    }

    /// Closed the active block and move the writer to `Wait` state.
    fn close_current(&mut self) -> io::Result<()> {
        let (mut stream, block_id) = match mem::replace(&mut self.state, BlockState::Invalid) {
            BlockState::Wait(stream) => (stream, !0),

            BlockState::Active {
                writer, block_id, ..
            } => (writer.into_stream()?, block_id),

            BlockState::Invalid => unreachable!(),
        };

        // Compute how bytes have been written to the stream and update the
        // block length (4 bytes, big-endian) at the beginning of it.
        let current_position = stream.stream_position()?;
        let len = current_position - (block_id + /* tag */ 1 + /* length */ 4);

        if len > 0 {
            let len_bytes = u32::try_from(len)
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::Other, "block size can't be written as u32")
                })?
                .to_be_bytes();

            stream.seek(SeekFrom::Start(block_id + 1))?;
            stream.write_all(&len_bytes)?;
            stream.seek(SeekFrom::Start(current_position))?;
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
    pub(crate) fn fragment(&mut self, size_hint: u64) -> io::Result<Fragment<impl Write + '_>> {
        let current_offset = match &self.state {
            BlockState::Active { offset, .. } => *offset,
            _ => 0,
        };

        if size_hint == u64::MAX
            || (current_offset + size_hint > MAX_DATA_BLOCK_SIZE && current_offset > 0)
        {
            self.close_current()?;
        }

        // Change to `Active` state if it is waiting.
        //
        // Every block starts with the byte-tag, and the length (u32).
        if let BlockState::Wait(_) = self.state {
            match mem::replace(&mut self.state, BlockState::Invalid) {
                BlockState::Wait(mut stream) => {
                    let block_id = stream.stream_position()?;

                    stream.write_all(&[self.compression.tag() as u8, 0, 0, 0, 0])?;

                    let writer = match self.compression {
                        BlockCompression::None => Writer::Raw(stream),

                        #[cfg(feature = "deflate")]
                        BlockCompression::Deflate(level) => {
                            let encoder =
                                DeflateEncoder::new(stream, flate2::Compression::new(level));
                            Writer::Deflate(encoder)
                        }

                        #[cfg(feature = "lz4")]
                        BlockCompression::Lz4 => {
                            let encoder = lz4_flex::frame::FrameEncoder::new(stream);
                            Writer::Lz4(encoder)
                        }
                    };

                    self.state = BlockState::Active {
                        writer,
                        block_id,
                        offset: 0,
                    };
                }

                _ => unreachable!(),
            }
        }

        // Extract data from the state.
        match &mut self.state {
            BlockState::Active {
                writer,
                block_id,
                offset,
            } => {
                let offset_copy = *offset;
                let fragment = Fragment {
                    writer: writer.get_stream(),
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
    writer: S,

    writer_offset: &'a mut u64,

    block_id: u64,

    offset: u64,
}

/// Location to get a fragment.
pub(crate) struct FragmentLocation {
    pub(crate) block_id: u64,
    pub(crate) offset: u64,
}

impl<S> Fragment<'_, S> {
    /// Finish this fragment and returns its location.
    pub(crate) fn location(self) -> FragmentLocation {
        FragmentLocation {
            block_id: self.block_id,
            offset: self.offset,
        }
    }
}

impl<S: Write> Write for Fragment<'_, S> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        let n = self.writer.write(buf)?;
        *self.writer_offset += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.writer.flush()
    }
}
