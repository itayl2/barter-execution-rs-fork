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
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;
use uuid::Uuid;
use crate::simulated::execution::SimulatedExecution;


#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Record {
    pub update_id: u64,
    pub best_bid_price: f64,
    pub best_bid_qty: f64,
    pub best_ask_price: f64,
    pub best_ask_qty: f64,
    pub transaction_time: u64,
    pub event_time: u64,
}


#[derive(Clone)]
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

pub async fn run_default_exchange(
    event_account_tx: mpsc::UnboundedSender<AccountEvent>,
    event_simulated_rx: mpsc::UnboundedReceiver<SimulatedEvent>,
    event_simulated_tx: mpsc::UnboundedSender<SimulatedEvent>,
    balances: Option<ClientBalances>,
    live_trading: Arc<AtomicBool>,
    instrument: Instrument,
    records: &Vec<Record>,
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
    let simulated_execution_client = SimulatedExecution {
        request_tx: event_simulated_tx.clone(),
    };

    // Build SimulatedExchange & run on it's own Tokio task
    let mut sim_exchange = SimulatedExchange::builder()
        .event_simulated_rx(event_simulated_rx)
        .execution_client(simulated_execution_client)
        .account(account)
        .live_trading(live_trading)
        .event_waiting(event_waiting)
        .build()
        .expect("failed to build SimulatedExchange");

    let mut exchange_clone = sim_exchange.clone();
    tokio::spawn(async move {
        let _ = exchange_clone.run().await;
        eprintln!("sim_exchange.run() finished");
    });

    tokio::spawn(async move {
        let _ = sim_exchange.load_fast(instrument.clone(), records, None).await;
        eprintln!("sim_exchange.load_past_data() finished");
    });

    loop {
        sleep(Duration::from_secs(1)).await;
    }
    eprintln!("sleep finished");

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
    vec![Instrument::from(("btc", "usdt", InstrumentKind::Perpetual))]
}

// Initial SimulatedExchange ClientAccount balances for each Symbol
pub fn initial_balances() -> ClientBalances {
    ClientBalances(HashMap::from([
        (Symbol::from("btc"), Balance::new(10.0, 10.0)),
        (Symbol::from("usdt"), Balance::new(10_000.0, 10_000.0)),
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
