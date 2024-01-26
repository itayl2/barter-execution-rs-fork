// file copied from {root}/tests/util to make these functions accessible for imports by other repos
// removed all (super) mentions to make the functions accessbile to the public
use bincode;
use crate::{
    model::{
        balance::Balance,
        order::{Cancelled, Open, Order, OrderId, OrderKind, RequestCancel, RequestOpen},
        AccountEvent, ClientOrderId,
    },
    simulated::{
        exchange::{
            account::{balance::ClientBalances, ClientAccount},
            SimulatedExchange,
        },
        SimulatedEvent,
    },
    ExecutionId,
};
use barter_integration::model::{
    instrument::{kind::InstrumentKind, symbol::Symbol, Instrument},
    Exchange, Side,
};
use std::{collections::HashMap, time::Duration};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use barter_data::subscription::trade::PublicTrade;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, MutexGuard};
use tokio::time::{Instant, sleep};
use uuid::{Builder, Uuid, Variant, Version};
use crate::simulated::exchange::{simulated_exchange_load_fast, simulated_exchange_load_slow, simulated_exchange_run};
use crate::simulated::execution::SimulatedExecution;


#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Record {
    pub update_id: u64,
    pub best_bid_price: f64,
    pub best_bid_qty: f64,
    pub best_ask_price: f64,
    pub best_ask_qty: f64,
    pub transaction_time: u64,
    pub event_time: u64,
}

pub struct RecordOrders {
    pub buy: Order<RequestOpen>,
    pub sell: Order<RequestOpen>,
}

impl Record {
    const SELL_UUID_PREFIX: u8 = 0xa;
    const BUY_UUID_PREFIX: u8 = 0xb;
    const DEFAULT_QUANTITY: f64 = 100000000.0;

    pub fn to_market_trade(&self, target_order: &TargetOrder) -> PublicTrade {
        PublicTrade {
            id: target_order.id.cid.to_string(),
            price: target_order.price,
            amount: target_order.quantity,
            side: target_order.side,
        }
    }

    pub fn to_limit_order(&self, side: Side, instrument: Instrument) -> Order<RequestOpen> {
        order_request_limit(
            instrument,
            ClientOrderId(Uuid::new_v4()),
            side,
            self.get_price(side),
            100000000.0,
        )
    }

    pub fn get_shared_uuid_suffix() -> [u8; 15] {
        let mut random_bytes_suffix = [0u8; 15];
        getrandom::getrandom(&mut random_bytes_suffix).unwrap_or_else(|err| {
            panic!("could not retrieve random bytes for uuid: {}", err)
        });
        random_bytes_suffix
    }

    pub fn get_buy_and_sell_order_requests(&self, instrument: Instrument) -> RecordOrders {
        let shared_suffix = Self::get_shared_uuid_suffix();
        let mut sell_uuid_bytes = [Self::SELL_UUID_PREFIX; 16];
        let mut buy_uuid_bytes = [Self::BUY_UUID_PREFIX; 16];
        sell_uuid_bytes[1..].copy_from_slice(&shared_suffix);
        buy_uuid_bytes[1..].copy_from_slice(&shared_suffix);

        let sell_uuid = Builder::from_bytes(sell_uuid_bytes)
            .set_variant(Variant::RFC4122)
            .set_version(Version::Random)
            .build();
        let buy_uuid = Builder::from_bytes(buy_uuid_bytes)
            .set_variant(Variant::RFC4122)
            .set_version(Version::Random)
            .build();

        RecordOrders {
            buy: order_request_limit(
                instrument.clone(),
                ClientOrderId(sell_uuid),
                Side::Sell,
                self.best_ask_price,
                Self::DEFAULT_QUANTITY
            ),
            sell: order_request_limit(
                instrument,
                ClientOrderId(buy_uuid),
                Side::Buy,
                self.best_bid_price,
                Self::DEFAULT_QUANTITY,
            )
        }
    }

    pub fn get_price(&self, side: Side) -> f64 {
        match side {
            Side::Buy => self.best_bid_price,
            Side::Sell => self.best_ask_price,
        }
    }
}

impl PartialEq for Record {
    fn eq(&self, other: &Self) -> bool {
        self.best_bid_price == other.best_bid_price &&
            self.best_ask_price == other.best_ask_price &&
            self.transaction_time == other.transaction_time &&
            self.event_time == other.event_time
    }
}


#[derive(Clone, Deserialize, Serialize)]
pub struct Ids {
    pub cid: ClientOrderId,
    pub id: OrderId,
}

impl Ids {
    pub fn new<Id: Into<OrderId>>(cid: Uuid, id: Id) -> Self {
        Self {
            cid: ClientOrderId(cid),
            id: id.into(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TargetOrder {
    pub id: Ids,
    pub price: f64,
    pub quantity: f64,
    pub side: Side,
}

pub struct ClientTargetOrders {
    pub buy: Vec<TargetOrder>,
    pub sell: Vec<TargetOrder>
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CopyAbleTargetOrder {
    pub id: String,
    pub price: f64,
    pub quantity: f64,
    pub side: Side,
}

impl CopyAbleTargetOrder {
    pub fn from_original(order: TargetOrder) -> CopyAbleTargetOrder {
        CopyAbleTargetOrder {
            id: serde_json::to_string(&order.id)
                .expect("Failed to parse Ids to JSON"),
            price: order.price,
            quantity: order.quantity,
            side: order.side,
        }
    }
}

pub struct CopyAbleClientTargetOrders {
    pub buy: Vec<CopyAbleTargetOrder>,
    pub sell: Vec<CopyAbleTargetOrder>
}

impl TargetOrder {

    pub fn new(id: Ids, price: f64, quantity: f64, side: Side) -> Self {
        Self {
            id,
            price,
            quantity,
            side,
        }
    }
    pub fn matches_record(&self, record: &Record) -> bool {
        match self.side {
            Side::Buy => self.price >= record.best_ask_price,
            Side::Sell => self.price <= record.best_bid_price,
        }
    }

    pub fn from_copyable(order: CopyAbleTargetOrder) -> TargetOrder {
        let ids: Ids = serde_json::from_str(&order.id)
            .expect("Failed to parse JSON to Ids");

        TargetOrder {
            id: ids,
            price: order.price,
            quantity: order.quantity,
            side: order.side,
        }
    }

    pub fn to_copyable(&self) -> CopyAbleTargetOrder {
        CopyAbleTargetOrder {
            id: serde_json::to_string(&self.id)
                .expect("Failed to parse Ids to JSON"),
            price: self.price,
            quantity: self.quantity,
            side: self.side,
        }
    }
}

impl ClientTargetOrders {
    pub fn from_mutex(client_orders: &MutexGuard<CopyAbleClientTargetOrders>) -> ClientTargetOrders {
        ClientTargetOrders {
            buy: client_orders.buy.clone().into_iter().map(|order| TargetOrder::from_copyable(order)).collect(),
            sell: client_orders.sell.clone().into_iter().map(|order| TargetOrder::from_copyable(order)).collect(),
        }
    }

    pub fn to_copyable(&self) -> CopyAbleClientTargetOrders {
        CopyAbleClientTargetOrders {
            buy: self.buy.iter().map(|order| order.to_copyable()).collect(),
            sell: self.sell.iter().map(|order| order.to_copyable()).collect(),
        }
    }
}

pub async fn run_default_exchange(
    event_account_tx: mpsc::UnboundedSender<AccountEvent>,
    event_simulated_rx: mpsc::UnboundedReceiver<SimulatedEvent>,
    event_simulated_tx: mpsc::UnboundedSender<SimulatedEvent>,
    balances: Option<ClientBalances>,
    live_trading: Arc<AtomicBool>,
    instrument: Instrument,
    records: &Vec<Record>,
    target_prices: Arc<Mutex<CopyAbleClientTargetOrders>>,
) -> Result<(), Box<dyn Error>> {
    // Define SimulatedExchange available Instruments
    let instruments = instruments();

    // Create initial ClientAccount balances (Symbols must all be included in the Instruments)
    let final_balances = balances.unwrap_or(initial_balances());
    let account = Arc::new(Mutex::new(ClientAccount::builder()
        .latency(latency_50ms())
        .fees_percent(fees_50_percent())
        .event_account_tx(event_account_tx)
        .instruments(instruments)
        .balances(final_balances)
        .build()
        .expect("failed to build ClientAccount")));


    let event_waiting = Arc::new(AtomicBool::new(false));
    let event_waiting_clone = Arc::clone(&event_waiting);
    let mut event_waiting_load_fast = Arc::clone(&event_waiting);
    let mut event_waiting_load_slow = Arc::clone(&event_waiting);

    let mut live_trading_load_fast = Arc::clone(&live_trading);
    let mut live_trading_load_slow = Arc::clone(&live_trading);

    let mut event_simulated_rx = event_simulated_rx;
    let account_clone2 = Arc::clone(&account);
    let records_clone = records.clone();
    let target_prices_clone = Arc::clone(&target_prices);

    // Build SimulatedExchange & run on it's own Tokio task
    tokio::spawn(async move {
        let mut simulated_execution_client = SimulatedExecution {
            request_tx: event_simulated_tx.clone(),
        };
        let records_count = records_clone.len();
        let mut current_records_index = 0;
        let total_time = Instant::now();
        while current_records_index < records_count {
            if let Err(e) = simulated_exchange_load_fast(account.clone(), instrument.clone(), &records_clone, &mut current_records_index, &mut event_waiting_load_fast, &mut live_trading_load_fast, Arc::clone(&target_prices_clone)).await {
                eprintln!("Failed simulated_exchange_load_fast at index {}: {e:?}", current_records_index);
                break;
            }

            if current_records_index < records_count {
                if let Err(e) = simulated_exchange_load_slow(instrument.clone(), &records_clone, &mut current_records_index, &mut simulated_execution_client, &mut event_waiting_load_slow, &mut live_trading_load_slow).await {
                    eprintln!("Failed simulated_exchange_load_slow at index {}: {e:?}", current_records_index);
                    break;
                }
            }
        }
        println!("Done loading records into exchange, took: {} seconds", total_time.elapsed().as_secs());
    });

    simulated_exchange_run(account_clone2, &mut event_simulated_rx, event_waiting_clone).await;

    Ok(())
}

pub fn deserialize_records_from_binary(path: &Path) -> Result<Vec<Record>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let records: Vec<Record> = bincode::deserialize_from(reader)?;
    Ok(records)
}

pub fn latency_50ms() -> Duration {
    Duration::from_millis(50)
}

pub fn fees_50_percent() -> f64 {
    0.5
}

// Instruments that the SimulatedExchange supports
pub fn instruments() -> Vec<Instrument> {
    vec![Instrument::from(("btc", "usd", InstrumentKind::Perpetual))]
}

// Initial SimulatedExchange ClientAccount balances for each Symbol
pub fn initial_balances() -> ClientBalances {
    ClientBalances(HashMap::from([
        (Symbol::from("btc"), Balance::new(10.0, 10.0)),
        (Symbol::from("usd"), Balance::new(10_000.0, 10_000.0)),
    ]))
}

// Utility for creating an Open Order request
pub fn order_request_limit<I>(
    instrument: I,
    cid: ClientOrderId,
    side: Side,
    price: f64,
    quantity: f64,
) -> Order<RequestOpen>
    where
        I: Into<Instrument>,
{
    Order {
        exchange: Exchange::from(ExecutionId::Simulated),
        instrument: instrument.into(),
        cid,
        side,
        state: RequestOpen {
            kind: OrderKind::Limit,
            price,
            quantity,
        },
    }
}

// Utility for creating an Open Order
pub fn open_order<I>(
    instrument: I,
    cid: ClientOrderId,
    id: OrderId,
    side: Side,
    price: f64,
    quantity: f64,
    filled: f64,
) -> Order<Open>
    where
        I: Into<Instrument>,
{
    Order {
        exchange: Exchange::from(ExecutionId::Simulated),
        instrument: instrument.into(),
        cid,
        side,
        state: Open {
            id,
            price,
            quantity,
            filled_quantity: filled,
        },
    }
}

// Utility for creating an Order RequestCancel
pub fn order_cancel_request<I, Id>(
    instrument: I,
    cid: ClientOrderId,
    side: Side,
    id: Id,
) -> Order<RequestCancel>
    where
        I: Into<Instrument>,
        Id: Into<OrderId>,
{
    Order {
        exchange: Exchange::from(ExecutionId::Simulated),
        instrument: instrument.into(),
        cid,
        side,
        state: RequestCancel::from(id),
    }
}

// Utility for creating an Order<Cancelled>
pub fn order_cancelled<I, Id>(
    instrument: I,
    cid: ClientOrderId,
    side: Side,
    id: Id,
) -> Order<Cancelled>
    where
        I: Into<Instrument>,
        Id: Into<OrderId>,
{
    Order {
        exchange: Exchange::from(ExecutionId::Simulated),
        instrument: instrument.into(),
        cid,
        side,
        state: Cancelled::from(id),
    }
}
