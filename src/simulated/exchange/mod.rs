use barter_integration::model::instrument::Instrument;
use barter_integration::model::Side;
use super::{exchange::account::ClientAccount, SimulatedEvent};
use crate::{ExecutionClient, ExecutionError};
use tokio::sync::{mpsc, Mutex, MutexGuard};
use crate::simulated::SimulatedEvent::FetchFirstBidAndAsk;
use std::sync::atomic::{AtomicBool, Ordering};
use core::slice::Iter;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
use crate::simulated::exchange::account::order::Orders;
use crate::simulated::execution::SimulatedExecution;
use async_recursion::async_recursion;
use barter_data::subscription::trade::PublicTrade;
use tokio::time::{Instant, sleep};
use crate::util::{Ids, order_request_limit, Record, ClientTargetOrders, TargetOrder, CopyAbleClientTargetOrders};

/// [`SimulatedExchange`] account balances, open orders, fees, and latency.
pub mod account;

/// [`SimulatedExchange`] that responds to [`SimulatedEvent`]s.
#[derive(Debug)]
pub struct SimulatedExchange {}


pub async fn simulated_exchange_run(
    account: Arc<Mutex<ClientAccount>>,
    event_simulated_rx: &mut mpsc::UnboundedReceiver<SimulatedEvent>,
    event_waiting: Arc<AtomicBool>,
) {
    while let Some(event) = event_simulated_rx.recv().await {
        event_waiting.store(true, Ordering::SeqCst); // let the data-submission thread know that you are waiting for the account lock so it could release the lock and submit data via events while you are working
        let mut account_lock: MutexGuard<'_, ClientAccount> = account.lock().await;
        event_waiting.store(false, Ordering::SeqCst);

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
                let elapsed_micros = start_time.elapsed().as_micros();
                if elapsed_micros > 300 {
                    // println!("OpenOrdersNoBalance took: {:?} microseconds", elapsed_micros);
                }
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
                account_lock.match_orders(instrument, trade)
            }
            FetchFirstBidAndAsk((instrument, response_tx)) => {
                account_lock.get_first_bid_and_ask(&instrument, response_tx);
            }
        }
    }
}

pub async fn simulated_exchange_load_slow(
    instrument: Instrument,
    records: &Vec<Record>,
    current_index: &mut usize,
    execution_client: &mut SimulatedExecution,
    event_waiting: &mut Arc<AtomicBool>,
    live_trading: &mut Arc<AtomicBool>,
) -> Result<(), ExecutionError> {
    println!("GOING SLOWWWWW");
    let mut counter = 0;
    let total_records = records.len();
    let mut current_record = &records[*current_index];
    let mut current_event_time = current_record.event_time;
    let ping_print_interval = 10000;
    let mut ping_time = Instant::now();
    while *current_index < total_records {
        counter += 1;
        let record = &records[*current_index];
        if record == current_record {
            *current_index += 1;
            continue;
        }
        current_record = record;
        let order_requests = record.get_buy_and_sell_order_requests(instrument.clone());

        if live_trading.load(Ordering::SeqCst) {
            let delta_time = record.event_time - current_event_time;
            tokio::time::sleep(Duration::from_micros(delta_time)).await;
        } else if !event_waiting.load(Ordering::SeqCst) { // if both are false it means direct submit is available again
            let progress = *current_index as f64 / total_records as f64;
            println!("STOPPING SLOW: {ping_print_interval}, time: {}, progress: {:.2}%, record: {record:?}", ping_time.elapsed().as_millis(), progress);
            break;
        }

        execution_client.open_orders_no_balance_no_return(vec![order_requests.buy.clone(), order_requests.sell.clone()]).await?;
        current_event_time = record.event_time;
        *current_index += 1;
        counter += 1;
        if counter >= ping_print_interval {
            counter = 0;
            let progress = *current_index as f64 / total_records as f64;
            println!("SLOW: {ping_print_interval}, time: {}, progress: {:.2}%, record: {record:?}", ping_time.elapsed().as_millis(), progress);
            ping_time = Instant::now();
        }
    }

    Ok(())
}


pub async fn simulated_exchange_load_fast(
    account: Arc<Mutex<ClientAccount>>,
    instrument: Instrument,
    records: &Vec<Record>,
    current_index: &mut usize,
    event_waiting: &mut Arc<AtomicBool>,
    live_trading: &mut Arc<AtomicBool>,
    target_prices: Arc<Mutex<CopyAbleClientTargetOrders>>
) -> Result<(), ExecutionError> {
    println!("GOING FASTTT");
    let target_prices_owned = target_prices.lock().await;
    let mut parsed_target_orders = ClientTargetOrders::from_mutex(&target_prices_owned);
    drop(target_prices_owned);
    let mut concat_target_orders = parsed_target_orders.buy.clone().iter().chain(parsed_target_orders.sell.iter()).cloned().collect::<Vec<TargetOrder>>();

    let mut account_lock: MutexGuard<'_, ClientAccount> = account.lock().await;
    let total_records = records.len();
    let mut current_record = &records[*current_index];
    let mut current_event_time = current_record.event_time;
    let mut counter = 0;
    let ping_print_interval = 10000;
    let mut ping_time = Instant::now();
    let mut real_time_submission= false;
    while *current_index < total_records {
        let record = &records[*current_index];
        if record == current_record {
            *current_index += 1;
            continue;
        }
        let delta_time = record.event_time - current_event_time;
        current_record = record;
        let order_requests = record.get_buy_and_sell_order_requests(instrument.clone());

        if event_waiting.load(Ordering::SeqCst) || live_trading.load(Ordering::SeqCst) { // meaning we cannot use direct submit anymore
            let sort_time = Instant::now();
            account_lock.orders.orders_mut(&instrument)?.bids.sort();
            account_lock.orders.orders_mut(&instrument)?.asks.sort();
            let sort_elapsed = sort_time.elapsed().as_millis();

            drop(account_lock);
            let progress = *current_index as f64 / total_records as f64;
            println!("STOPPING FAST: {ping_print_interval}, time: {}, progress: {:.2}%, sort_elapsed: {sort_elapsed}ms, record: {record:?}", ping_time.elapsed().as_millis(), progress);
            break;
        }

        if delta_time > 0 {
            sleep(Duration::from_millis(delta_time)).await;
        }

        if let Some(index) = concat_target_orders.iter().position(|order| order.matches_record(record)) {
            let matched_target_order = concat_target_orders[index].clone();
            // concat_target_orders.remove(index);

            let mut sort_elapsed: u128 = 0;
            if real_time_submission {
                let sort_time = Instant::now();
                account_lock.orders.orders_mut(&instrument)?.bids.sort();
                account_lock.orders.orders_mut(&instrument)?.asks.sort();
                sort_elapsed = sort_time.elapsed().as_millis();
            } else {
                account_lock.orders.orders_mut(&instrument)?.bids = vec![]; // so that it matches only with ours?
                account_lock.orders.orders_mut(&instrument)?.asks = vec![]; // so that it matches only with ours?
            }

            println!("Matched target order: {:?} with {record:?}, sort_elapsed: {sort_elapsed}", matched_target_order.to_copyable());
            account_lock.match_orders(instrument.clone(), record.to_market_trade(&matched_target_order));
            real_time_submission = true;
        }
        account_lock.orders.build_buy_and_sell_open_and_add(order_requests.buy.clone(), order_requests.sell.clone(), &instrument)?;
        current_event_time = record.event_time;
        *current_index += 1;
        counter += 1;
        if counter >= ping_print_interval {
            counter = 0;
            let progress = *current_index as f64 / total_records as f64;
            println!("FAST: {ping_print_interval}, time: {}, progress: {:.2}%, record: {record:?}", ping_time.elapsed().as_millis(), progress);
            ping_time = Instant::now();
        }
    }
    Ok(())
}

impl SimulatedExchange {
    /// Construct a [`ExchangeBuilder`] for configuring a new [`SimulatedExchange`].
    pub fn builder() -> ExchangeBuilder {
        ExchangeBuilder::new()
    }

    /// Run the [`SimulatedExchange`] by responding to [`SimulatedEvent`]s.
    pub async fn run(
        mut self,
        account: Arc<Mutex<ClientAccount>>,
        event_simulated_rx: &mut mpsc::UnboundedReceiver<SimulatedEvent>,
        event_waiting: Arc<AtomicBool>,
    ) {
        while let Some(event) = event_simulated_rx.recv().await {
            event_waiting.store(true, Ordering::SeqCst);
            let mut account_lock: MutexGuard<'_, ClientAccount> = account.lock().await;
            event_waiting.store(false, Ordering::SeqCst);

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
                    // println!("OpenOrdersNoBalance took: {:?} microseconds", start_time.elapsed().as_micros());
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
                    account_lock.match_orders(instrument, trade)
                }
                FetchFirstBidAndAsk((instrument, response_tx)) => {
                    account_lock.get_first_bid_and_ask(&instrument, response_tx);
                }
            }
        }
    }
    //
    // #[async_recursion]
    // pub async fn load_slow(
    //     &mut self,
    //     account: Arc<Mutex<ClientAccount>>,
    //     instrument: Instrument,
    //     records: &Vec<Record>,
    //     start_index: Option<usize>,
    //     execution_client: SimulatedExecution,
    //     event_waiting: Arc<AtomicBool>,
    //     live_trading: Arc<AtomicBool>,
    // ) -> Result<(), ExecutionError> {
    //     let total_records = records.len();
    //     let mut current_index = start_index.unwrap_or(0);
    //     let mut current_event_time = records[current_index].event_time;
    //     while current_index < total_records {
    //         // TODO maybe don't submit if current is identical to previous in bid price & ask price & event_time & transaction_time
    //         let record = &records[current_index];
    //         let limit_buy_request = order_request_limit(
    //             instrument.clone(),
    //             Ids::new(Uuid::new_v4(), record.update_id.to_string()).cid,
    //             Side::Buy,
    //             record.best_bid_price,
    //             100000000.0,
    //         );
    //
    //         if live_trading.load(Ordering::SeqCst) {
    //             let delta_time = record.event_time - current_event_time;
    //             tokio::time::sleep(tokio::time::Duration::from_micros(delta_time)).await;
    //         } else if !event_waiting.load(Ordering::SeqCst) { // if both are false it means direct submit is available again
    //             return self.load_fast(account, instrument, records, Some(current_index), execution_client, event_waiting, live_trading).await;
    //         }
    //
    //         execution_client.open_orders_no_balance_no_return(vec![limit_buy_request]).await?;
    //         current_event_time = record.event_time;
    //         current_index += 1;
    //     }
    //
    //     Ok(())
    // }
    //
    // // once we submit a trade from our trading client, we'll acquire a lock on self.account.
    // // this means no new orders will be submitted while we are mutating self.account for the purpose of trading.
    // // This is basically another way to look at the issue wherein this sim exchange is single threaded - it listens to events in a single loop, so it handles one person at a time.
    // // This might create situations where we were able to buy at a price that only lasted single milliseconds, for example.
    // // TODO To be safe, when we review the data after sims / backtests, we should check how long our exit price / buy price was viable when we executed it.
    // #[async_recursion]
    // pub async fn load_fast(
    //     &mut self,
    //     account: Arc<Mutex<ClientAccount>>,
    //     instrument: Instrument,
    //     records: &Vec<Record>,
    //     start_index: Option<usize>,
    //     execution_client: SimulatedExecution,
    //     event_waiting: Arc<AtomicBool>,
    //     live_trading: Arc<AtomicBool>,
    // ) -> Result<(), ExecutionError> {
    //     let mut account_lock: MutexGuard<'_, ClientAccount> = account.lock().await;
    //     // let orders = account_lock.orders.orders_mut(&instrument)?;
    //     let total_records = records.len();
    //     let mut current_index = start_index.unwrap_or(0);
    //     let mut current_event_time = records[current_index].event_time;
    //     while current_index < total_records {
    //         // TODO maybe don't submit if current is identical to previous in bid price & ask price & event_time & transaction_time
    //         let record = &records[current_index];
    //         let limit_buy_request = order_request_limit(
    //             instrument.clone(),
    //             Ids::new(Uuid::new_v4(), record.update_id.to_string()).cid,
    //             Side::Buy,
    //             record.best_bid_price,
    //             100000000.0,
    //         );
    //
    //         let limit_buy_open = account_lock.orders.build_order_open(limit_buy_request);
    //         if event_waiting.load(Ordering::SeqCst) || live_trading.load(Ordering::SeqCst) { // meaning we cannot use direct submit anymore
    //             drop(account_lock);
    //             return self.load_slow(account, instrument, records, Some(current_index), execution_client, event_waiting, live_trading).await;
    //         }
    //
    //         // orders.add_order_open(limit_buy_open.clone());
    //         account_lock.orders.orders_mut(&instrument)?.add_order_open(limit_buy_open.clone());
    //         current_event_time = record.event_time;
    //         current_index += 1;
    //     }
    //     Ok(())
    // }
}

#[derive(Debug, Default)]
pub struct ExchangeBuilder {}

impl ExchangeBuilder {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn build(self) -> Result<SimulatedExchange, ExecutionError> {
        Ok(SimulatedExchange {})
    }
}
