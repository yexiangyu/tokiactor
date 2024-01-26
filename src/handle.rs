use anyhow::Context;
use async_channel as channel;
use tokio::sync::oneshot;

use crate::{
    actor::{ActorFuture, ActorFutureFn},
    error::{HandleError, HandleResult},
};

pub struct Handle<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub handle: channel::Sender<(I, oneshot::Sender<O>)>,
}

impl<I, O> Clone for Handle<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
        }
    }
}

impl<I, O> Handle<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub async fn try_send(&self, i: I) -> HandleResult<oneshot::Receiver<O>> {
        let (tx, rx) = oneshot::channel::<O>();
        self.handle
            .send((i, tx))
            .await
            .map_err(|_| HandleError::SendError)?;
        Ok(rx)
    }

    pub async fn try_handle(&self, i: I) -> HandleResult<O> {
        Ok(self
            .try_send(i)
            .await?
            .await
            .map_err(|_| HandleError::RecvError)?)
    }

    pub async fn handle(&self, i: I) -> O {
        self.try_handle(i).await.expect("failed to handle")
    }

    pub fn then<ON>(self, rhs: Handle<O, ON>) -> Handle<I, ON>
    where
        ON: Send + 'static,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move {
                let o = ll.handle(i).await;
                rr.handle(o).await
            }
        })
        .spawn(1)
    }

    pub fn join<U, V>(self, rhs: Handle<U, V>) -> Handle<(I, U), (O, V)>
    where
        U: Send + 'static,
        V: Send + 'static,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |(i, u): (I, U)| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move { futures::join!(ll.handle(i), rr.handle(u)) }
        })
        .spawn(1)
    }

    pub fn fork<V>(self, rhs: Handle<I, V>) -> Handle<I, (O, V)>
    where
        I: Clone,
        V: Send + 'static,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move { futures::join!(ll.handle(i.clone()), rr.handle(i)) }
        })
        .spawn(1)
    }
}

pub type ResultHandle<I, O, E> = Handle<I, Result<O, E>>;

impl<I, O, E> ResultHandle<I, O, E>
where
    I: Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
{
    pub fn ok(self) -> OptionHandle<I, O> {
        let ll = self;
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            async move { ll.handle(i).await.ok() }
        })
        .spawn(1)
    }

    pub fn map<ON>(self, rhs: Handle<O, ON>) -> ResultHandle<I, ON, E>
    where
        ON: Send + 'static,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move {
                let o = ll.handle(i).await?;
                let on = rr.handle(o).await;
                Result::Ok(on)
            }
        })
        .spawn(1)
    }

    pub fn and<U, V, EN>(
        self,
        rhs: ResultHandle<U, V, EN>,
    ) -> ResultHandle<(I, U), (O, V), anyhow::Error>
    where
        U: Send + 'static,
        V: Send + 'static,
        E: Sync + std::error::Error,
        EN: Sync + Send + 'static + std::error::Error,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |(i, u): (I, U)| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move {
                let (o, v) = futures::join!(ll.handle(i), rr.handle(u));
                anyhow::Result::Ok((o?, v?))
            }
        })
        .spawn(1)
    }

    pub fn and_then<ON, EN>(
        self,
        rhs: ResultHandle<O, ON, EN>,
    ) -> ResultHandle<I, ON, anyhow::Error>
    where
        ON: Send + 'static,
        E: Sync + std::error::Error,
        EN: Sync + Send + 'static + std::error::Error,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move {
                let o = ll.handle(i).await?;
                let on = rr.handle(o).await?;
                anyhow::Result::Ok(on)
            }
        })
        .spawn(1)
    }
}

pub type OptionHandle<I, O> = Handle<I, Option<O>>;

impl<I, O> OptionHandle<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub fn context<C>(self, context: C) -> ResultHandle<I, O, anyhow::Error>
    where
        C: ToString,
    {
        let ll = self;
        let context = context.to_string();
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            let context = context.clone();
            async move { ll.handle(i).await.context(context) }
        })
        .spawn(1)
    }

    pub fn map<ON>(self, rhs: Handle<O, ON>) -> OptionHandle<I, ON>
    where
        ON: Send + 'static,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move {
                match ll.handle(i).await {
                    Some(o) => Some(rr.handle(o).await),
                    None => None,
                }
            }
        })
        .spawn(1)
    }

    pub fn and<U, V>(self, rhs: OptionHandle<U, V>) -> OptionHandle<(I, U), (O, V)>
    where
        U: Send + 'static,
        V: Send + 'static,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |(i, u): (I, U)| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move {
                let (o, v) = futures::join!(ll.handle(i), rr.handle(u));
                match (o, v) {
                    (Some(o), Some(v)) => Some((o, v)),
                    _ => None,
                }
            }
        })
        .spawn(1)
    }

    pub fn and_then<ON>(self, rhs: OptionHandle<O, ON>) -> OptionHandle<I, ON>
    where
        ON: Send + 'static,
    {
        let ll = self;
        let rr = rhs;
        ActorFutureFn::new(move |i: I| {
            let ll = ll.clone();
            let rr = rr.clone();
            async move {
                match ll.handle(i).await {
                    Some(o) => rr.handle(o).await,
                    None => None,
                }
            }
        })
        .spawn(1)
    }
}
