use std::sync::Arc;
use std::task::Poll;

use covert_types::error::ApiError;
use futures::future::BoxFuture;
use hyper::StatusCode;
use tokio::sync::mpsc;
use tokio::sync::{self, Notify};
use tower::{Service, ServiceExt};
use tracing_error::SpanTrace;

struct Message<Req, Res, Err> {
    request: Req,
    tx: sync::oneshot::Sender<Result<Res, Err>>,
    notify: Arc<Notify>,
}

#[derive(Debug)]
pub struct SyncService<Req, Res> {
    tx: mpsc::UnboundedSender<Message<Req, Res, ApiError>>,
}

impl<Req, Res> Clone for SyncService<Req, Res> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<Req, Res> SyncService<Req, Res> {
    pub fn new<T>(service: T) -> Self
    where
        T: Service<Req, Response = Res, Error = ApiError> + Send + Clone + 'static,
        T::Future: Send,
        Req: Send + 'static,
        Res: Send + 'static,
    {
        let (tx, mut rx) = mpsc::unbounded_channel::<Message<Req, Res, ApiError>>();

        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let svc = service.clone();
                let resp = svc.oneshot(message.request).await;
                if message.tx.send(resp).is_err() {
                    tracing::error!(
                        "Failed to notify sync service of the response from the worker"
                    );
                }
                message.notify.notify_one();
            }
        });

        Self { tx }
    }
}

impl<Req, Res> Service<Req> for SyncService<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    type Response = Res;

    type Error = ApiError;

    type Future = BoxFuture<'static, Result<Res, ApiError>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            let (tx, mut rx) = sync::oneshot::channel();
            let notify = Arc::new(Notify::new());
            this.tx
                .send(Message {
                    request: req,
                    tx,
                    notify: notify.clone(),
                })
                .map_err(|_| ApiError {
                    error: anyhow::Error::msg("Internal error")
                        .context("Unable to send message to worker in sync service"),
                    status_code: StatusCode::INTERNAL_SERVER_ERROR,
                    span_trace: Some(SpanTrace::capture()),
                })?;

            notify.notified().await;

            rx.try_recv().map_err(|_| ApiError {
                error: anyhow::Error::msg("Internal error")
                    .context("Unable to receive message from worker in sync service"),
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                span_trace: Some(SpanTrace::capture()),
            })?
        })
    }
}

// THE TEST

trait Handler: Send + Sync {}

impl<Req: Send, Res: Send> Handler for SyncService<Req, Res> {}

struct PostgresBackend(SyncService<hyper::Request<hyper::Body>, hyper::Response<hyper::Body>>);

impl Handler for PostgresBackend {}
