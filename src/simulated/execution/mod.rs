use crate::{
    model::order::{Cancelled, Open, Order},
    simulated::SimulatedEvent,
    AccountEvent, ExecutionClient, ExecutionError, ExecutionId, RequestCancel, RequestOpen,
    SymbolBalance,
};
use async_trait::async_trait;
use barter_integration::model::instrument::Instrument;
use tokio::sync::{mpsc, oneshot};
use anyhow::Error;

/// Simulated [`ExecutionClient`] implementation that integrates with the Barter
/// [`SimulatedExchange`](super::exchange::SimulatedExchange).
#[derive(Clone, Debug)]
pub struct SimulatedExecution {
    pub request_tx: mpsc::UnboundedSender<SimulatedEvent>,
}

#[async_trait]
impl ExecutionClient for SimulatedExecution {
    const CLIENT: ExecutionId = ExecutionId::Simulated;
    type Config = mpsc::UnboundedSender<SimulatedEvent>;

    async fn init(request_tx: Self::Config, _: mpsc::UnboundedSender<AccountEvent>) -> Self {
        Self { request_tx }
    }

    async fn fetch_orders_open(&self) -> Result<Vec<Order<Open>>, ExecutionError> {
        // Oneshot channel to communicate with the SimulatedExchange
        let (response_tx, response_rx) = oneshot::channel();

        // Send FetchOrdersOpen request to the SimulatedExchange
        self.request_tx
            .send(SimulatedEvent::FetchOrdersOpen(response_tx))
            .expect("SimulatedExchange is offline - failed to send FetchOrdersOpen request");

        // Receive FetchOrdersOpen response from the SimulatedExchange
        response_rx
            .await
            .expect("SimulatedExchange is offline - failed to receive FetchOrdersOpen response")
    }

    async fn fetch_balances(&self) -> Result<Vec<SymbolBalance>, ExecutionError> {
        // Oneshot channel to communicate with the SimulatedExchange
        let (response_tx, response_rx) = oneshot::channel();

        // Send FetchBalances request to the SimulatedExchange
        self.request_tx
            .send(SimulatedEvent::FetchBalances(response_tx))
            .expect("SimulatedExchange is offline - failed to send FetchBalances request");

        // Receive FetchBalances response from the SimulatedExchange
        response_rx
            .await
            .expect("SimulatedExchange is offline - failed to receive FetchBalances response")
    }

    async fn open_orders(
        &self,
        open_requests: Vec<Order<RequestOpen>>,
    ) -> Vec<Result<Order<Open>, ExecutionError>> {
        // Oneshot channel to communicate with the SimulatedExchange
        let (response_tx, response_rx) = oneshot::channel();

        // Send OpenOrders request to the SimulatedExchange
        self.request_tx
            .send(SimulatedEvent::OpenOrders((open_requests, response_tx)))
            .expect("SimulatedExchange is offline - failed to send OpenOrders request");

        // Receive OpenOrders response from the SimulatedExchange
        response_rx
            .await
            .expect("SimulatedExchange is offline - failed to receive OpenOrders response")
    }

    async fn open_orders_no_balance_no_return(
        &self,
        open_requests: Vec<Order<RequestOpen>>,
    ) -> Result<(), ExecutionError> {
        // Oneshot channel to communicate with the SimulatedExchange
        let (response_tx, response_rx) = oneshot::channel();

        // Send OpenOrders request to the SimulatedExchange
        self.request_tx
            .send(SimulatedEvent::OpenOrdersNoBalance((open_requests, response_tx)))
            .expect("SimulatedExchange is offline - failed to send OpenOrders request");

        // Receive OpenOrders response from the SimulatedExchange
        response_rx
            .await
            .expect("SimulatedExchange is offline - failed to receive OpenOrders response");

        Ok(())
    }

    fn open_orders_no_response_no_balance(
        &self,
        open_requests: Vec<Order<RequestOpen>>,
    ) -> Result<(), ExecutionError> {
        self.request_tx
            .send(SimulatedEvent::OpenOrdersNoResponseNoBalance((open_requests)))
            .expect("SimulatedExchange is offline - failed to send OpenOrders request");
        Ok(())
    }

    async fn cancel_orders(
        &self,
        cancel_requests: Vec<Order<RequestCancel>>,
    ) -> Vec<Result<Order<Cancelled>, ExecutionError>> {
        // Oneshot channel to communicate with the SimulatedExchange
        let (response_tx, response_rx) = oneshot::channel();

        // Send CancelOrders request to the SimulatedExchange
        self.request_tx
            .send(SimulatedEvent::CancelOrders((cancel_requests, response_tx)))
            .expect("SimulatedExchange is offline - failed to send CancelOrders request");

        // Receive CancelOrders response from the SimulatedExchange
        response_rx
            .await
            .expect("SimulatedExchange is offline - failed to receive CancelOrders response")
    }

    async fn cancel_orders_all(&self) -> Result<Vec<Order<Cancelled>>, ExecutionError> {
        // Oneshot channel to communicate with the SimulatedExchange
        let (response_tx, response_rx) = oneshot::channel();

        // Send CancelOrdersAll request to the SimulatedExchange
        self.request_tx
            .send(SimulatedEvent::CancelOrdersAll(response_tx))
            .expect("SimulatedExchange is offline - failed to send CancelOrdersAll request");

        // Receive CancelOrdersAll response from the SimulatedExchange
        response_rx
            .await
            .expect("SimulatedExchange is offline - failed to receive CancelOrdersAll response")
    }

    async fn fetch_first_bid_and_ask(&self, instrument: Instrument) -> Result<(Option<f64>, Option<f64>), ExecutionError> {
        // Oneshot channel to communicate with the SimulatedExchange
        let (response_tx, response_rx) = oneshot::channel();

        // Send FetchFirstBidAndAsk request to the SimulatedExchange
        self.request_tx
            .send(SimulatedEvent::FetchFirstBidAndAsk((instrument, response_tx)))
            .expect("SimulatedExchange is offline - failed to send FetchFirstBidAndAsk request");

        println!("Sent a request for first bid and ask");
        // Receive FetchFirstBidAndAsk response from the SimulatedExchange
        match response_rx.await {
            Ok(response) => {
                println!("Got response: {:?}", response);
                response
            },
            Err(e) => {
                eprintln!("SimulatedExchange is offline - failed to receive FetchFirstBidAndAsk response: {:?}", e);
                Ok((Some(0.0), Some(0.0)))
            }
        }
    }
}
