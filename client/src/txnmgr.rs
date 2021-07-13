//!
//! Manages transaction.
//!
#![warn(missing_docs)]

use bitcoin::Txid;
use core::time;
use std::fmt::Debug;
use thiserror::Error;

use crate::handle;

/// The status of a transaction.
#[derive(Clone, Debug)]
pub enum Event {
    /// The transaction was sent to one or more peers on the network, and is unconfirmed.
    Pending {
        /// Number of announcements of this transaction from other peers.
        announcements: usize,
        /// Transaction hash.
        txid: Txid,
    },
    /// The transaction has been accepted by one or more peers on the network.
    Accepted {
        /// Number of peers that our transaction data was sent to.
        confirmations: usize,
        /// Transaction hash.
        txid: Txid,
    },
}

/// Transaction related error.
#[derive(Debug, Error)]
pub enum Error {
    /// Error due to RwLock.
    Lock,
    /// Error due to unavailable relay peers.
    RelayPeer,
    /// Transaction not in store.
    NotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Lock => write!(fmt, "Poisonsed read/write lock"),
            Error::RelayPeer => write!(fmt, "Zero connected relay peers"),
            Error::NotFound => write!(fmt, "Transaction not stored to store"),
        }
    }
}

impl From<Error> for handle::Error {
    fn from(e: Error) -> Self {
        Self::Transaction(e)
    }
}

impl std::fmt::Display for Event {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Pending {
                announcements,
                txid,
            } => {
                write!(
                    fmt,
                    "Pending transaction ID {} broadcasted to {} peers",
                    txid, announcements
                )
            }
            Event::Accepted {
                confirmations,
                txid,
            } => {
                write!(
                    fmt,
                    "Transaction with ID {} sent to {} peers",
                    confirmations, txid,
                )
            }
        }
    }
}

/// Transaction status event.
pub trait Events {
    /// Emit a transaction-related event.
    fn event(&self, event: Event);
}

/// Trait for sending and tracking transactions.
pub trait Transaction {
    /// Submit transaction to peers.
    fn submit_transaction(
        &mut self,
        txn: bitcoin::Transaction,
        duration: time::Duration,
    ) -> Result<Event, handle::Error>;
    /// Wait for transaction to be sent to a peer.
    fn wait(&self, tx_id: Txid, timeout: time::Duration) -> Result<Event, handle::Error>;
}
