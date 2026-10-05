//! Validated domain values for Quota.
//!
//! This crate holds the pure model: identities, quota windows, measurements,
//! and the least-remaining-first ranking rule. It has no Tauri, React, Tokio,
//! network, or database dependency, and it denies unsafe code.

#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod account;
pub mod balance;
pub mod error;
pub mod ids;
pub mod percent;
pub mod polling;
pub mod preferences;
pub mod provider;
pub mod quota;
pub mod ranking;
pub mod snapshot;

pub use error::DomainError;
pub use percent::Percent;
pub use ranking::{AccountOrder, RankedOrder, UnrankedOrder, UnrankedReason};
pub use snapshot::{AccountSnapshot, AppSnapshot};
