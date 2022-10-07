use crate::BlockCompression;
use std::io::{Cursor, Read, Write};

#[test]
fn write_read() {
    write_read_with_compression(BlockCompression::None);

    #[cfg(feature = "deflate")]
    write_read_with_compression(BlockCompression::Deflate(0));

    #[cfg(feature = "deflate")]
    write_read_with_compression(BlockCompression::Deflate(6));
}

fn write_read_with_compression(compression: BlockCompression) {
    let mut buffer = Vec::new();

    let mut writer = Cursor::new(&mut buffer);
    writer.write_all(&b"<prefix>"[..]).unwrap();

    let mut writer = super::DataBlocksWriter::new(writer, compression);

    // First fragment: 50×'A' + 50×'B'
    let mut fragment = writer.fragment(100).unwrap();
    fragment.write_all(&[b'A'; 50]).unwrap();
    fragment.write_all(&[b'B'; 50]).unwrap();

    let block1 = fragment.location();
    assert_eq!(block1.offset, 0);

    // Second fragment: 10×'C'
    let mut fragment = writer.fragment(10).unwrap();
    fragment.write_all(&[b'C'; 10]).unwrap();
    let fragment = fragment.location();

    assert_eq!(block1.block_id, fragment.block_id);
    assert_eq!(fragment.offset, 100);

    // Third fragment: 10×'D', but use a very big number as the hint.
    let mut fragment = writer.fragment(0xFFFFFF).unwrap();
    fragment.write_all(&[b'D'; 10]).unwrap();

    let block2 = fragment.location();
    assert_eq!(block2.offset, 0);

    writer.finish().unwrap();

    // Check the written data.
    let mut reader = Cursor::new(&buffer);

    // The prefix should be kept.
    let mut prefix = [0; 8];
    reader.read_exact(&mut prefix).unwrap();
    assert_eq!(&prefix, b"<prefix>");

    let mut reader = super::DataBlocksReader::new(reader).unwrap();

    // The first block contains the ABC sequences.
    let expected = {
        let mut bytes = vec![0; 110];
        bytes[0..50].fill(b'A');
        bytes[50..100].fill(b'B');
        bytes[100..110].fill(b'C');
        bytes
    };

    let block_bytes = reader
        .with_block(block1.block_id, 0, |b| Vec::from(b))
        .unwrap();
    assert_eq!(block_bytes, expected);

    // The second block contains 10×'D'.
    let expected = vec![b'D'; 10];
    let block_bytes = reader
        .with_block(block2.block_id, 0, |b| Vec::from(b))
        .unwrap();
    assert_eq!(block_bytes, expected);
}
