#![warn(
    missing_debug_implementations,
    missing_copy_implementations,
    rust_2018_idioms,
    // missing_docs
)]

///! # Barter-Execution
///! Todo:

use crate::{
    error::ExecutionError,
    model::{
        AccountEvent,
        balance::SymbolBalance,
        order::{Order, RequestOpen, RequestCancel},
    }
};
use tokio::sync::mpsc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Contains `ExchangeClient` implementations for specific exchanges.
pub mod exchange;
/// Todo:
pub mod error;
pub mod model;
pub mod builder;

// Todo:
//  - Add Health/ClientStatus

#[async_trait]
pub trait ExecutionClient {
    type Config;
    async fn init(config: Self::Config, event_tx: mpsc::UnboundedSender<AccountEvent>) -> Self;
    async fn fetch_orders_open(&self) -> Result<Vec<Order<()>>, ExecutionError>;
    async fn fetch_balances(&self) -> Result<Vec<SymbolBalance>, ExecutionError>;
    async fn open_orders(&self, open_requests: Vec<Order<RequestOpen>>) -> Result<Vec<Order<()>>, ExecutionError>;
    async fn cancel_orders(&self, cancel_requests: Vec<Order<RequestCancel>>) -> Result<Vec<Order<()>>, ExecutionError>;
    async fn cancel_orders_all(&self) -> Result<Vec<Order<()>>, ExecutionError>;
}

/// Used to uniquely identify an [`ExecutionClient`] implementation.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
#[serde(rename = "client", rename_all = "snake_case")]
pub enum ClientId {
    Simulated,
    Ftx,
}