use barter_integration::model::instrument::Instrument;
use barter_integration::model::Side;
use super::{exchange::account::ClientAccount, SimulatedEvent};
use crate::ExecutionError;
use tokio::sync::mpsc;
use crate::simulated::SimulatedEvent::FetchFirstBidAndAsk;

/// [`SimulatedExchange`] account balances, open orders, fees, and latency.
pub mod account;

/// [`SimulatedExchange`] that responds to [`SimulatedEvent`]s.
#[derive(Debug)]
pub struct SimulatedExchange {
    pub event_simulated_rx: mpsc::UnboundedReceiver<SimulatedEvent>,
    pub account: ClientAccount,
    pub last_buy_price: Option<f64>,
    pub last_sell_price: Option<f64>,
}

impl SimulatedExchange {
    /// Construct a [`ExchangeBuilder`] for configuring a new [`SimulatedExchange`].
    pub fn builder() -> ExchangeBuilder {
        ExchangeBuilder::new()
    }

    /// Run the [`SimulatedExchange`] by responding to [`SimulatedEvent`]s.
    pub async fn run(mut self) {
        while let Some(event) = self.event_simulated_rx.recv().await {
            match event {
                SimulatedEvent::FetchOrdersOpen(response_tx) => {
                    self.account.fetch_orders_open(response_tx)
                }
                SimulatedEvent::FetchBalances(response_tx) => {
                    self.account.fetch_balances(response_tx)
                }
                SimulatedEvent::OpenOrders((open_requests, response_tx)) => {
                    self.account.open_orders(open_requests, response_tx)
                }
                SimulatedEvent::OpenOrdersNoBalance((open_requests, response_tx)) => {
                    let start_time = std::time::Instant::now();
                    self.account.open_orders_no_balance_no_latency(open_requests, response_tx);
                    println!("OpenOrdersNoBalance took: {:?}", start_time.elapsed().as_millis());
                }
                SimulatedEvent::OpenOrdersNoResponseNoBalance((open_requests)) => {
                    self.account.open_orders_no_response_no_balance(open_requests)
                }
                SimulatedEvent::CancelOrders((cancel_requests, response_tx)) => {
                    self.account.cancel_orders(cancel_requests, response_tx)
                }
                SimulatedEvent::CancelOrdersAll(response_tx) => {
                    self.account.cancel_orders_all(response_tx)
                }
                SimulatedEvent::MarketTrade((instrument, trade)) => {
                    match trade.side {
                        Side::Buy => {
                            self.last_buy_price = Some(trade.price.clone());
                        }
                        Side::Sell => {
                            self.last_sell_price = Some(trade.price.clone());
                        }
                    }
                    self.account.match_orders(instrument, trade)
                }
                FetchFirstBidAndAsk((instrument, response_tx)) => {
                    self.account.get_first_bid_and_ask(&instrument, response_tx);
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ExchangeBuilder {
    event_simulated_rx: Option<mpsc::UnboundedReceiver<SimulatedEvent>>,
    account: Option<ClientAccount>,
}

impl ExchangeBuilder {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn event_simulated_rx(self, value: mpsc::UnboundedReceiver<SimulatedEvent>) -> Self {
        Self {
            event_simulated_rx: Some(value),
            ..self
        }
    }

    pub fn account(self, value: ClientAccount) -> Self {
        Self {
            account: Some(value),
            ..self
        }
    }

    pub fn build(self) -> Result<SimulatedExchange, ExecutionError> {
        Ok(SimulatedExchange {
            event_simulated_rx: self.event_simulated_rx.ok_or_else(|| {
                ExecutionError::BuilderIncomplete("event_simulated_rx".to_string())
            })?,
            account: self
                .account
                .ok_or_else(|| ExecutionError::BuilderIncomplete("account".to_string()))?,
            last_buy_price: Some(0.0),
            last_sell_price: Some(0.0),
        })
    }
}
