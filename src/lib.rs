//! Xenstore implementation for Rust.
//!
//! Xenstore is a shared database protocol for Xen domains used for
//! Xen PV devices (xenbus), guest agents, toolstack, ...
//!
//! Check docs/misc/xenstore.txt in xen source code for detailed informations.

pub(crate) mod wire;

#[cfg(feature = "unix")]
pub mod unix;

#[cfg(feature = "async-tokio")]
pub mod tokio;

use std::io;

/// Xenstore base trait.
/// All xenstore implementations must implement this trait.
pub trait Xs {
    /// Try to list the files of a directory.
    fn directory(&self, path: &str) -> io::Result<Vec<Box<str>>>;

    /// Read a node.
    fn read(&self, path: &str) -> io::Result<Box<str>>;

    /// Write a node.
    fn write(&self, path: &str, data: &str) -> io::Result<()>;

    /// Remove a node.
    fn rm(&self, path: &str) -> io::Result<()>;
}

/// Xenstore transaction capability trait.
///
/// A transaction can be created with [XsTransaction::transaction] as a [XsTransactionSpan].
/// This span can be used to make operation within this transaction and be effective
/// only if the transaction is commited ([XsTransactionSpan::commit]).
///
/// If you want to discard the transaction and make its changes effective, use
/// [XsTransactionSpan::commit].
///
/// # Drop
///
/// [Drop] is called on a transaction, it is aborted.
pub trait XsTransaction: Xs {
    type Span: Xs; // + 'static ?

    fn transaction(&self) -> io::Result<Self::Span>;
}

/// Refer to [XsTransaction] for more information.
pub trait XsTransactionSpan: Xs {
    /// Commit a transaction.
    fn commit(self) -> io::Result<()>;
}

/// [`Xs`] async variant.
#[cfg(feature = "async")]
#[trait_variant::make(AsyncXs: Send)]
pub trait LocalAsyncXs {
    /// Try to list the files of a directory.
    async fn directory(&self, path: &str) -> io::Result<Vec<Box<str>>>;

    /// Read a node.
    async fn read(&self, path: &str) -> io::Result<Box<str>>;

    /// Write a node.
    async fn write(&self, path: &str, data: &str) -> io::Result<()>;

    /// Remove a node.
    async fn rm(&self, path: &str) -> io::Result<()>;
}

/// [`XsTransaction`] async variant.
#[cfg(feature = "async")]
#[trait_variant::make(AsyncXsTransaction: Send)]
pub trait LocalAsyncXsTransaction: AsyncXs {
    type Span: Xs;

    async fn transaction(&self) -> io::Result<Self::Span>;
}

/// [`XsTransactionSpan`] async variant.
#[cfg(feature = "async")]
#[trait_variant::make(AsyncXsTransactionSpan: Send)]
pub trait LocalAsyncXsTransactionSpan: Xs {
    /// Commit a transaction.
    async fn commit(self) -> io::Result<()>;
}

/// Xenstore watch capability trait.
#[cfg(feature = "async")]
#[trait_variant::make(AsyncWatch: Send)]
pub trait LocalAsyncWatch {
    /// Create a [`futures::Stream`] yielding paths of updated nodes/subnodes.
    async fn watch(
        &self,
        path: &str,
    ) -> io::Result<impl futures::Stream<Item = Box<str>> + Unpin + 'static>;
}
