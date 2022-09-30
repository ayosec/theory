//! Persistence for pages.
//!
//! # Storage Format
//!
//! Pages are stored in two parts: an index and a collection of *data blocks*.
//!
//! The data stored at the [`pages_pos`] position in the stream in the page
//! index. Each entry is composed by `6` numbers:
//!
//! 1. Page identifier.
//! 2. Identifier of the parent page, or `0` if none.
//! 3. Data block with the metadata.
//! 4. Offset in the data block for the metadata.
//! 5. Data black with the page content.
//! 6. Offset in the data block for the page content.
//!
//! Each number is encoded as a 4 bytes, big-endian, unsigned integer. The total
//! size of each entry is `24` bytes.
//!
//! [`pages_pos`]: crate::Package::pages_pos

use std::borrow::Cow;
use std::io::{self, Cursor, Read, Seek, Write};
use std::num::NonZeroU32;

use crate::persistence::datablock::{DataBlocksReader, DataBlocksWriter};
use crate::{page, persistence::kvlist, Page};

use endiannezz::Io;

/// Tag for the metadata entries for each page.
#[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive, Debug, Copy, Clone)]
#[repr(u8)]
pub(crate) enum ByteTag {
    Title = 1,
    Keywords = 2,
    Description = 3,
}

#[derive(Debug)]
struct MetadataEntry<'a>(ByteTag, Cow<'a, str>);

impl kvlist::VariantValue for MetadataEntry<'_> {
    type Key = ByteTag;

    type DeserializeError = std::string::FromUtf8Error;

    fn serialize(&self) -> (Self::Key, kvlist::InnerValue) {
        (self.0, kvlist::InnerValue::Slice(self.1.as_bytes()))
    }

    fn deserialize(key: Self::Key, bytes: Vec<u8>) -> Result<Self, Self::DeserializeError> {
        Ok(MetadataEntry(key, String::from_utf8(bytes)?.into()))
    }
}

macro_rules! to_u32 {
    ($e:expr) => {
        match u32::try_from($e) {
            Ok(n) => n,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Content is too large.",
                ))
            }
        }
    };
}

/// A single entry in the page index.
#[derive(Io, Debug)]
#[endian(big)]
pub(super) struct IndexEntry {
    /// Page identifier.
    pub(super) id: u32,

    /// Identifier of the parent page, or `0` if none.
    pub(super) parent_id: u32,

    /// Data block with the metadata.
    pub(super) metadata_block_id: u32,

    /// Offset in the data block for the metadata.
    pub(super) metadata_block_offset: u32,

    /// Data black with the page content.
    pub(super) content_block_id: u32,

    /// Offset in the data block for the page content.
    pub(super) content_block_offset: u32,
}

/// Write the page table and the data block in the output stream.
///
/// On success, returns the offset to the page index.
pub(crate) fn dump_pages<'a, O, P, I>(output: O, pages: I) -> io::Result<u64>
where
    O: Write + Seek,
    P: Into<&'a Page>,
    I: IntoIterator<Item = P>,
{
    // To reduce the seek operations, the page index is written in memory, while
    // the data blocks are written to the stream.
    //
    // All metadata is written in the same data block.

    let pages = pages.into_iter();

    let mut metadata_buf = Vec::with_capacity(4 * 1024);
    let mut page_index = Vec::with_capacity(pages.size_hint().0);

    let mut db_writer = DataBlocksWriter::new(output);

    for page in pages.map(|e| e.into()) {
        // Content is written directly to the output stream.
        let content = page.content.as_deref().unwrap_or("").as_bytes();
        let mut fragment = db_writer.fragment(content.len() as u64)?;

        leb128::write::unsigned(&mut fragment, content.len() as u64)?;
        fragment.write_all(content)?;

        let content_block_id = to_u32!(fragment.block_id());
        let content_block_offset = to_u32!(fragment.offset());

        // Metadata
        let metadata_block_offset = to_u32!(metadata_buf.len());
        let entries: Vec<_> = [
            (ByteTag::Title, Some(page.title.as_ref())),
            (ByteTag::Keywords, page.keywords.as_deref()),
            (ByteTag::Description, page.description.as_deref()),
        ]
        .iter()
        .filter_map(|(tag, value)| value.map(|v| MetadataEntry(*tag, Cow::Borrowed(v))))
        .collect();

        kvlist::serialize(&mut metadata_buf, &entries)?;

        // Page index.
        //
        // `metadata_block_id` is updated after `metadata_buf` is written.
        page_index.push(IndexEntry {
            id: to_u32!(page.id),
            parent_id: page.parent_id.map(|id| id.get()).unwrap_or(0),
            metadata_block_id: !0,
            metadata_block_offset,
            content_block_id,
            content_block_offset,
        });
    }

    // Send the metadata to the output.
    let mut fragment_metadata = db_writer.fragment(u64::MAX)?;
    fragment_metadata.write_all(&metadata_buf)?;

    let metadata_block_id = to_u32!(fragment_metadata.block_id());

    let mut output = db_writer.finish()?;

    // Write the index.
    let page_index_position = output.stream_position()?;
    for mut page in page_index {
        page.metadata_block_id = metadata_block_id;
        page.write(&mut output)?;
    }

    Ok(page_index_position)
}

/// Build a `Page` value using the data from a stream.
pub(super) fn build_page<R>(
    entry: &IndexEntry,
    db_reader: &mut DataBlocksReader<R>,
) -> Result<Page, page::Error>
where
    R: Read + Seek,
{
    // Page content.
    let content = db_reader.with_block(
        entry.content_block_id.into(),
        |bytes: &[u8]| -> Result<_, page::Error> {
            let content_block_offset = entry.content_block_offset as usize;
            let mut cursor = match bytes.get(content_block_offset..) {
                Some(slice) => Cursor::new(slice),
                None => return Err(page::Error::InvalidLength(content_block_offset as u64)),
            };

            let len = leb128::read::unsigned(&mut cursor)? as usize;
            let position = cursor.position() as usize + content_block_offset;
            let bytes = match bytes.get(position..len + position) {
                Some(bytes) => bytes,
                None => return Err(page::Error::InvalidLength(len as u64)),
            };

            Ok(String::from_utf8(bytes.to_owned())?)
        },
    )??;

    // Page metadata.
    let mut title = None;
    let mut keywords = None;
    let mut description = None;

    db_reader.with_block(
        entry.metadata_block_id.into(),
        |bytes: &[u8]| -> Result<_, page::Error> {
            let metadata_block_offset = entry.metadata_block_offset as usize;
            let cursor = match bytes.get(metadata_block_offset..) {
                Some(slice) => Cursor::new(slice),
                None => return Err(page::Error::InvalidLength(metadata_block_offset as u64)),
            };

            let input_len = cursor.get_ref().len() as u64;
            for metadata_entry in kvlist::deserialize(cursor, input_len) {
                let MetadataEntry(tag, value) =
                    metadata_entry.map_err(|e| page::Error::InvalidMetadata(e.to_string()))?;

                let value = value.into_owned();

                match tag {
                    ByteTag::Title => title = Some(value),
                    ByteTag::Keywords => keywords = Some(value),
                    ByteTag::Description => description = Some(value),
                }
            }

            Ok(())
        },
    )??;

    // Final page.
    let page = Page {
        id: NonZeroU32::new(entry.id).ok_or(page::Error::InvalidId(0))?,
        parent_id: NonZeroU32::new(entry.parent_id),
        title: title.unwrap_or_default(),
        keywords,
        description,
        content: Some(content),
    };

    Ok(page)
}
