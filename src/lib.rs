//! This crate provides a library to store and load documentation content in a
//! single file.
//!
//! Content is organized into *pages*. The collection of pages is a *book*. Both
//! pages and books can contain metadata entries.
//!
//! # Books
//!
//! The main type in the crate is [`Book`]. A [`Book`] may contains multiple
//! [pages](crate::Page) and [metadata entries](crate::MetadataEntry).
//!
//! ## Creating a New Book
//!
//! New books are created with [`BookBuilder`].
//!
//! Content is added with [`add_metadata`](BookBuilder::add_metadata) and
//! [`new_page`](BookBuilder::new_page).
//!
//! When the book is completed, it can be persisted with
//! [`dump`](BookBuilder::dump).
//!
//! Optionally, data can be compressed with
//! [`set_compression`](BookBuilder::set_compression).
//!
//! ### Example
//!
//! ```
//! use theory::{MetadataEntry, Book, Page};
//! use std::io::{Cursor, Read, Write};
//!
//! let mut buffer: Vec<u8> = Vec::new();
//!
//! let mut builder = Book::builder();
//! builder.new_page("First").set_content("1");
//! builder.new_page("Second").set_content("2");
//!
//! builder
//!     .add_metadata(MetadataEntry::Title("Theory Example".into()))
//!     .dump(Cursor::new(&mut buffer));
//!
//! let book = Book::load(Cursor::new(buffer)).unwrap();
//!
//! assert_eq!(book.num_pages(), 2);
//! ```
//!
//! ## Loading a Book
//!
//! A book written by [`BookBuilder::dump`] can be loaded with [`Book::load`].
//!
//! # Crate Features
//!
//! Features can be used for controlling some functionalities in the library:
//!
//! * `deflate`
//!
//!     Add supports for compressing books with
//!     [DEFLATE](https://en.wikipedia.org/wiki/Deflate).
//!
//! * `lz4`
//!
//!     Add supports for compressing books with
//!     [LZ4](https://en.wikipedia.org/wiki/LZ4_(compression_algorithm)).
//!
//! All features are enabled by default.

mod book;
mod metadata;
mod page;
mod toc;

pub(crate) mod builder;
pub(crate) mod persistence;

pub use book::Book;
pub use builder::BookBuilder;
pub use metadata::MetadataEntry;
pub use page::{Page, PageId};
pub use persistence::datablock::BlockCompression;
pub use toc::TocEntry;

/// Types to describe errors.
pub mod errors {
    pub use crate::metadata::MetadataError;
    pub use crate::page::PageError;
    pub use crate::persistence::PersistenceError;
    pub use crate::toc::TocError;
}
