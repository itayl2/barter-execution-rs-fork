use barter_integration::model::instrument::Instrument;
use barter_integration::model::Side;
use super::{exchange::account::ClientAccount, SimulatedEvent};
use crate::{ExecutionClient, ExecutionError};
use tokio::sync::{mpsc, Mutex, MutexGuard};
use crate::simulated::SimulatedEvent::FetchFirstBidAndAsk;
use std::sync::atomic::{AtomicBool, Ordering};
use core::slice::Iter;
use std::sync::Arc;
use uuid::Uuid;
use crate::simulated::exchange::account::order::Orders;
use crate::simulated::execution::SimulatedExecution;
use crate::util::{Ids, order_request_limit, Record};

/// [`SimulatedExchange`] account balances, open orders, fees, and latency.
pub mod account;

/// [`SimulatedExchange`] that responds to [`SimulatedEvent`]s.
#[derive(Debug)]
pub struct SimulatedExchange {
    pub event_simulated_rx: mpsc::UnboundedReceiver<SimulatedEvent>,
    pub account: Arc<Mutex<ClientAccount>>,
    pub last_buy_price: Option<f64>,
    pub last_sell_price: Option<f64>,
    pub execution_client: SimulatedExecution,
    pub live_trading: Arc<AtomicBool>,
    pub event_waiting: Arc<AtomicBool>
}

impl SimulatedExchange {
    /// Construct a [`ExchangeBuilder`] for configuring a new [`SimulatedExchange`].
    pub fn builder() -> ExchangeBuilder {
        ExchangeBuilder::new()
    }

    /// Run the [`SimulatedExchange`] by responding to [`SimulatedEvent`]s.
    pub async fn run(mut self) {
        while let Some(event) = self.event_simulated_rx.recv().await {
            self.event_waiting.store(true, Ordering::SeqCst);
            let mut account_lock: MutexGuard<'_, ClientAccount> = match self.account.lock().await {
                Ok(account_lock) => account_lock,
                Err(_) => {
                    eprintln!("SimulatedExchange account lock poisoned");
                    continue;
                }
            };
            self.event_waiting.store(false, Ordering::SeqCst);

            match event {
                SimulatedEvent::FetchOrdersOpen(response_tx) => {
                    account_lock.fetch_orders_open(response_tx)
                }
                SimulatedEvent::FetchBalances(response_tx) => {
                    account_lock.fetch_balances(response_tx)
                }
                SimulatedEvent::OpenOrders((open_requests, response_tx)) => {
                    account_lock.open_orders(open_requests, response_tx)
                }
                SimulatedEvent::OpenOrdersNoBalance((open_requests, response_tx)) => {
                    let start_time = std::time::Instant::now();
                    // let open = self.account.orders.build_order_open(open_requests[0].clone());

                    // Retrieve client Instrument Orders
                    // let orders = self.account.orders.orders_mut(&open.instrument)?;
                    account_lock.open_orders_no_balance_no_latency(open_requests, response_tx);
                    println!("OpenOrdersNoBalance took: {:?} microseconds", start_time.elapsed().as_micros());
                }
                SimulatedEvent::OpenOrdersNoResponseNoBalance((open_requests)) => {
                    account_lock.open_orders_no_response_no_balance(open_requests)
                }
                SimulatedEvent::CancelOrders((cancel_requests, response_tx)) => {
                    account_lock.cancel_orders(cancel_requests, response_tx)
                }
                SimulatedEvent::CancelOrdersAll(response_tx) => {
                    account_lock.cancel_orders_all(response_tx)
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
                    account_lock.match_orders(instrument, trade)
                }
                FetchFirstBidAndAsk((instrument, response_tx)) => {
                    account_lock.get_first_bid_and_ask(&instrument, response_tx);
                }
            }
        }
    }

    // once we submit a trade from our trading client, we'll acquire a lock on self.account.
    // this means no new orders will be submitted while we are mutating self.account for the purpose of trading.
    // This is basically another way to look at the issue wherein this sim exchange is single threaded - it listens to events in a single loop, so it handles one person at a time.
    // This might create situations where we were able to buy at a price that only lasted single milliseconds, for example.
    // TODO To be safe, when we review the data after sims / backtests, we should check how long our exit price / buy price was viable when we executed it.
    pub async fn load_past_data(
        &mut self,
        instrument: Instrument,
        records: &Vec<Record>,
        start_index: Option<usize>,
    ) -> Result<(), ExecutionError> {
        let execution_client = match self.execution_client {
            Some(ref execution_client) => execution_client,
            None => {
                eprintln!("SimulatedExchange execution_client is None");
                panic!("SimulatedExchange execution_client is None");
            }
        };
        let mut account_lock: MutexGuard<'_, ClientAccount>;
        let orders: &mut Orders;
        let mut direct_submit = false;
        if !self.event_waiting.load(Ordering::SeqCst) && !self.live_trading.load(Ordering::SeqCst) {
            account_lock = match self.account.lock().await {
                Ok(account_lock) => {
                    direct_submit = true;
                    orders = account_lock.orders.orders_mut(&instrument)?;
                    account_lock
                },
                Err(_) => {
                    eprintln!("SimulatedExchange account lock poisoned");
                    panic!("SimulatedExchange account lock poisoned");
                }
            }
        };
        let total_records = records.len();
        let mut current_index = start_index.unwrap_or(0);
        let mut current_event_time = records[current_index].event_time;
        while current_index < total_records {
            // TODO maybe don't submit if current is identical to previous in bid price & ask price & event_time & transaction_time
            let record = &records[current_index];
            let limit_buy_request = order_request_limit(
                instrument.clone(),
                Ids::new(Uuid::new_v4(), record.update_id.to_string()).cid,
                Side::Buy,
                record.best_bid_price,
                100000000.0,
            );

            if direct_submit {
                let limit_buy_open = account_lock.orders.build_order_open(limit_buy_request);
                if self.event_waiting.load(Ordering::SeqCst) || self.live_trading.load(Ordering::SeqCst) { // meaning we cannot use direct submit anymore
                    return self.load_past_data(instrument, records, Some(current_index)).await;
                }
                orders.add_order_open(limit_buy_open.clone());

            } else {
                if self.live_trading.load(Ordering::SeqCst) {
                    let delta_time = record.event_time - current_event_time;
                    tokio::time::sleep(tokio::time::Duration::from_micros(delta_time)).await;
                } else if !self.event_waiting.load(Ordering::SeqCst) { // if both are false it means direct submit is available again
                    return self.load_past_data(instrument, records, Some(current_index)).await;
                }

                *execution_client.open_orders_no_balance_no_return(vec![limit_buy_request]).await;
            }
            current_event_time = record.event_time;
            current_index += 1;
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ExchangeBuilder {
    event_simulated_rx: Option<mpsc::UnboundedReceiver<SimulatedEvent>>,
    execution_client: Option<SimulatedExecution>,
    account: Option<Arc<Mutex<ClientAccount>>>,
    live_trading: Option<Arc<AtomicBool>>,
    event_waiting: Option<Arc<AtomicBool>>
}

impl ExchangeBuilder {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn live_trading(self, value: Arc<AtomicBool>) -> Self {
        Self {
            live_trading: Some(value),
            ..self
        }
    }

    pub fn event_waiting(self, value: Arc<AtomicBool>) -> Self {
        Self {
            event_waiting: Some(value),
            ..self
        }
    }

    pub fn event_simulated_rx(self, value: mpsc::UnboundedReceiver<SimulatedEvent>) -> Self {
        Self {
            event_simulated_rx: Some(value),
            ..self
        }
    }
    pub fn execution_client(self, value: SimulatedExecution) -> Self {
        Self {
            execution_client: Some(value),
            ..self
        }
    }

    pub fn account(self, value: Arc<Mutex<ClientAccount>>) -> Self {
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
            execution_client: self.execution_client.ok_or_else(|| {
                ExecutionError::BuilderIncomplete("execution_client".to_string())
            })?,
            live_trading: self.live_trading.ok_or_else(|| {
                ExecutionError::BuilderIncomplete("live_trading".to_string())
            })?,
            event_waiting: self.event_waiting.ok_or_else(|| {
                ExecutionError::BuilderIncomplete("event_waiting".to_string())
            })?,
            account: self
                .account
                .ok_or_else(|| ExecutionError::BuilderIncomplete("account".to_string()))?,
            last_buy_price: Some(0.0),
            last_sell_price: Some(0.0),
        })
    }
}
