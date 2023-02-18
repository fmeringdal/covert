use std::task::Poll;

use covert_types::error::ApiError;
use futures::future::BoxFuture;
use tokio::sync::{mpsc, oneshot};
use tower::{Service, ServiceExt};

struct Message<Req, Res, Err> {
    request: Req,
    tx: oneshot::Sender<Result<Res, Err>>,
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
                // Ensure that a slow response does not block other requests
                tokio::spawn(async move {
                    let resp = svc.oneshot(message.request).await;
                    if message.tx.send(resp).is_err() {
                        tracing::error!(
                            "Failed to notify sync service of the response from the worker"
                        );
                    }
                });
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
            let (tx, rx) = oneshot::channel();
            this.tx
                .send(Message { request: req, tx })
                .map_err(|_| ApiError::internal_error())?;

            rx.await.map_err(|_| ApiError::internal_error())?
        })
    }
}

// THE TEST

trait Handler: Send + Sync {}

impl<Req: Send, Res: Send> Handler for SyncService<Req, Res> {}

struct PostgresBackend(SyncService<hyper::Request<hyper::Body>, hyper::Response<hyper::Body>>);

impl Handler for PostgresBackend {}
