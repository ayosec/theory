//! Theory.
//!
//! This crate is a *work-in-progress*.

mod book;
mod metadata;
mod page;

pub(crate) mod builder;
pub(crate) mod persistence;

pub use book::Book;
pub use builder::BookBuilder;
pub use metadata::MetadataEntry;
pub use page::Error as PageError;
pub use page::Page;
pub use persistence::Error as PersistenceError;
