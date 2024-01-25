// file copied from {root}/tests/util to make these functions accessible for imports by other repos
// removed all (super) mentions to make the functions accessbile to the public
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
use tokio::sync::mpsc;
use uuid::Uuid;


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
    balances: Option<ClientBalances>
) -> Result<(), Box<dyn Error>> {
    // Define SimulatedExchange available Instruments
    let instruments = instruments();

    // Create initial ClientAccount balances (Symbols must all be included in the Instruments)
    let final_balances = balances.unwrap_or(initial_balances());

    // Build SimulatedExchange & run on it's own Tokio task
    SimulatedExchange::builder()
        .event_simulated_rx(event_simulated_rx)
        .account(
            ClientAccount::builder()
                .latency(latency_50ms())
                .fees_percent(fees_50_percent())
                .event_account_tx(event_account_tx)
                .instruments(instruments)
                .balances(final_balances)
                .build()
                .expect("failed to build ClientAccount"),
        )
        .build()
        .expect("failed to build SimulatedExchange")
        .run()
        .await;
    Ok(())
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
