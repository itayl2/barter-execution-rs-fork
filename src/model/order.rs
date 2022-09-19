use super::ClientOrderId;
use barter_integration::model::{Exchange, Instrument, Side};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// Todo:
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Deserialize, Serialize)]
pub struct Order<State> {
    pub exchange: Exchange,
    pub instrument: Instrument,
    pub cid: ClientOrderId,
    pub state: State,
}

/// The initial state of an [`Order`]. Sent to the [`ExecutionClient`](crate::ExecutionClient) for
/// actioning.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct RequestOpen {
    pub kind: OrderKind,
    pub side: Side,
    pub price: f64,
    pub quantity: f64,
}

/// Type of [`Order`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub enum OrderKind {
    Market,
    Limit,
    PostOnly,
    ImmediateOrCancel,
}

impl Display for OrderKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                OrderKind::Market => "market",
                OrderKind::Limit => "limit",
                OrderKind::PostOnly => "post_only",
                OrderKind::ImmediateOrCancel => "immediate_or_cancel",
            }
        )
    }
}

/// State of an [`Order`] after a [`RequestOpen`] has been sent to the
/// [`ExecutionClient`](crate::ExecutionClient), but a confirmation response has not been received.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct InFlight;

/// State of an [`Order`] after a request has been made for it to be [`Cancelled`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct RequestCancel;

/// Todo:
#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct Open {
    pub id: OrderId,
    pub side: Side,
    pub price: f64,
    pub quantity: f64,
    pub filled_quantity: f64,
}

/// State of an [`Order`] after being [`Cancelled`].
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct Cancelled {
    id: OrderId,
}

/// [`Order`] identifier generated by an exchange. Cannot assume this is unique across each
/// [`Exchange`](barter_integration::model::Exchange),
/// [`Market`](barter_integration::model::Market), or
/// [`Instrument`](barter_integration::model::Instrument).
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct OrderId(pub String);

impl<S> From<S> for OrderId
where
    S: Into<String>,
{
    fn from(id: S) -> Self {
        Self(id.into())
    }
}

impl From<&Order<RequestOpen>> for Order<InFlight> {
    fn from(request: &Order<RequestOpen>) -> Self {
        Self {
            exchange: request.exchange.clone(),
            instrument: request.instrument.clone(),
            cid: request.cid,
            state: InFlight,
        }
    }
}