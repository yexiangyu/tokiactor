#![doc = include_str!("../README.md")]

use anyhow::Context;
use futures::Future;
use std::marker::PhantomData;

/// Actor to wrap function
pub struct Actor<I, O, FN>
where
    FN: FnMut(I) -> O,
{
    /// Function wrapped in actor
    pub f: FN,
    _i: PhantomData<I>,
    _o: PhantomData<O>,
}

impl<I, O, FN> Clone for Actor<I, O, FN>
where
    FN: FnMut(I) -> O + Clone,
{
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            _i: self._i,
            _o: self._o,
        }
    }
}

impl<I, O, FN> From<FN> for Actor<I, O, FN>
where
    FN: FnMut(I) -> O,
{
    fn from(f: FN) -> Self {
        Self {
            f,
            _i: PhantomData,
            _o: PhantomData,
        }
    }
}

/// Channel to communicated with `Actor` instances
pub struct Handle<I, O> {
    pub n: usize,
    tx: async_channel::Sender<(I, tokio::sync::oneshot::Sender<O>)>,
    rx: async_channel::Receiver<(I, tokio::sync::oneshot::Sender<O>)>,
}

impl<I, O> Clone for Handle<I, O> {
    fn clone(&self) -> Self {
        Self {
            n: self.n,
            tx: self.tx.clone(),
            rx: self.rx.clone(),
        }
    }
}

/// Error container for channel communication
#[derive(Debug, thiserror::Error)]
pub enum HandleError<T> {
    Send(#[from] async_channel::SendError<T>),
    Recv(#[from] tokio::sync::oneshot::error::RecvError),
}

/// A trait for inspecting input and output values.
pub trait Inspector<I, O> {
    /// Inspects the input value and returns it.
    fn inspect_i(&mut self, i: I) -> I {
        i
    }

    /// Inspects the output value and returns it.
    fn inspect_o(&mut self, o: O) -> O {
        o
    }
}

impl<I, O> Handle<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// create a new Handle with `n` of channel depth
    /// ```rust
    /// use tokiactor::*;
    ///
    /// let handle = Handle::<i32, i32>::new(1);
    /// ```
    pub fn new(n: usize) -> Self {
        let (tx, rx) = match n {
            0 => async_channel::unbounded(),
            _ => async_channel::bounded(n),
        };
        Self { n, tx, rx }
    }

    /// spawn `Actor` with type impl `Into<Actor<I, O, FN>>` in system thread
    /// ```rust
    /// use tokiactor::*;
    /// let handle = Handle::new(1).spawn(|i: i32| i + 1);
    /// ```
    pub fn spawn<FN, IntoActor>(self, actor: IntoActor) -> Handle<I, O>
    where
        FN: FnMut(I) -> O + Send + 'static,
        IntoActor: Into<Actor<I, O, FN>>,
    {
        let rx = self.rx.clone();
        let mut actor = actor.into();
        std::thread::spawn(move || {
            while let Ok((i, tx)) = rx.recv_blocking() {
                if tx.send((actor.f)(i)).is_err() {
                    break;
                }
            }
        });
        self
    }

    /// spawn `n` `Actor` with type impl `Into<Actor<I, O, FN>>` in system threads when `Actor` is `Clone`
    /// ```rust
    /// use tokiactor::*;
    /// let handle = Handle::new(1).spawn_n(10, |i: i32| i + 1);
    /// ```
    pub fn spawn_n<FN, IntoActor>(self, n: usize, actor: IntoActor) -> Handle<I, O>
    where
        FN: FnMut(I) -> O + Send + 'static,
        IntoActor: Into<Actor<I, O, FN>>,
        Actor<I, O, FN>: Clone,
    {
        let rx = self.rx.clone();
        let actor = actor.into();
        for _ in 0..n {
            let mut actor = actor.clone();
            let rx = rx.clone();
            std::thread::spawn(move || {
                while let Ok((i, tx)) = rx.recv_blocking() {
                    if tx.send((actor.f)(i)).is_err() {
                        break;
                    }
                }
            });
        }
        self
    }

    /// spawn async `Actor` with type impl `Into<Actor<I, O, FU, FN>>` in tokio thread
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn_tokio(move |i: i32| async move {i + 41})
    ///             .handle(1)
    ///             .await;
    ///         assert_eq!(r, 42)
    ///     }
    /// );
    /// ```
    pub fn spawn_tokio<FN, FU, IntoActor>(self, actor: IntoActor) -> Handle<I, O>
    where
        FN: FnMut(I) -> FU + Send + 'static,
        FU: Future<Output = O> + Send + 'static,
        IntoActor: Into<Actor<I, FU, FN>>,
    {
        let rx = self.rx.clone();
        let mut actor = actor.into();
        tokio::spawn(async move {
            let err = std::sync::Arc::from(std::sync::atomic::AtomicBool::new(false));
            while let Ok((i, tx)) = rx.recv().await {
                if err.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let o = (actor.f)(i);
                let err = err.clone();
                tokio::spawn(async move {
                    let o = o.await;
                    if tx.send(o).is_err() {
                        err.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });
        self
    }

    /// Handle input
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn_tokio(move |i: i32| async move {i + 41})
    ///             .handle(1)
    ///             .await;
    ///     }
    /// );
    /// ```
    pub async fn handle(&self, i: I) -> O {
        let (t, r) = tokio::sync::oneshot::channel::<O>();
        self.tx.send((i, t)).await.expect("failed to send to actor");
        r.await.expect("failed to recv from actor")
    }

    /// Try handle input, return error when actor is dead...
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         Handle::new(1)
    ///             .spawn_tokio(move |i: i32| async move {i + 41})
    ///             .try_handle(1)
    ///             .await;
    ///     }
    /// );
    /// ```
    pub async fn try_handle(
        &self,
        i: I,
    ) -> Result<O, HandleError<(I, tokio::sync::oneshot::Sender<O>)>> {
        let (t, r) = tokio::sync::oneshot::channel::<O>();
        self.tx.send((i, t)).await?;
        Ok(r.await?)
    }

    /// Inspect Handle excution, check `Inspector` trait for more info
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// use std::time::Instant;
    ///
    /// #[derive(Clone)]
    /// struct Timer(Instant);
    ///
    /// impl Inspector<i32, i32> for Timer
    /// {
    ///     fn inspect_i(&mut self, i: i32) -> i32
    ///     {
    ///         self.0 = Instant::now();
    ///         i
    ///     }
    ///
    ///     fn inspect_o(&mut self, o: i32) -> i32
    ///     {
    ///         let delta = self.0.elapsed();
    ///         // get execution time, do logging...
    ///         dbg!(delta);
    ///         o
    ///     }
    /// }
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         Handle::new(1)
    ///             .spawn_tokio(move |i: i32| async move {i + 41})
    ///             .inspect(Timer(Instant::now()))
    ///             .try_handle(1)
    ///             .await;
    ///     }
    /// );
    ///
    /// ```
    pub fn inspect<INS>(self, ins: INS) -> Handle<I, O>
    where
        INS: Inspector<I, O> + Send + 'static + Clone,
    {
        let ss = self;
        Handle::new(ss.n).spawn_tokio(Actor::from(move |i: I| {
            let ss = ss.clone();
            let mut ins = ins.clone();
            async move {
                let i = ins.inspect_i(i);
                let o = ss.handle(i).await;
                ins.inspect_o(o)
            }
        }))
    }

    ///convert `Handle<I, O>` to `Handle<I, P>` with `Fn(O)->P`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         Handle::new(1)
    ///             .spawn(move |i: i32| i + 1)
    ///             .convert(|v| v as f32);
    ///     }
    /// )
    /// ```
    pub fn convert<F, P>(self, f: F) -> Handle<I, P>
    where
        P: Send + 'static,
        F: Fn(O) -> P + Send + 'static + Clone,
    {
        let n = self.n;
        self.then(Handle::new(n).spawn_tokio(move |i| {
            let f = f.clone();
            async move { f(i) }
        }))
    }

    ///`Handle<I, O> + Handle<O, P> => Handle<I, P>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| i + 1)
    ///             .then(Handle::new(1).spawn(move |i: i32| i * 10)).handle(1).await;
    ///         assert_eq!(r, 20);
    ///     }
    /// )
    /// ```
    pub fn then<P>(self, rhs: Handle<O, P>) -> Handle<I, P>
    where
        P: Send + 'static,
    {
        let ss = self;
        let rr = rhs;
        Handle::new(ss.n).spawn_tokio(move |i| {
            let ss = ss.clone();
            let rr = rr.clone();
            async move {
                let o = ss.handle(i).await;
                rr.handle(o).await
            }
        })
    }

    /// `Handle<I, O> | Handle<U, V> => Handle<(I,U),(O,V)>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| i + 1)
    ///             .join(Handle::new(1).spawn(move |i: i32| i * 10)).handle((2, 1)).await;
    ///         assert_eq!(r, (3, 10));
    ///     }
    /// )
    /// ```
    pub fn join<U, V>(self, rhs: Handle<U, V>) -> Handle<(I, U), (O, V)>
    where
        U: Send + 'static,
        V: Send + 'static,
    {
        let ss = self;
        let rr = rhs;
        Handle::new(ss.n.max(rr.n)).spawn_tokio(Actor::from(move |(i, u)| {
            let ss = ss.clone();
            let rr = rr.clone();
            async move { futures::join!(ss.handle(i), rr.handle(u)) }
        }))
    }
}

/// Handle return value is an `Option`
pub type OptionHandle<I, O> = Handle<I, Option<O>>;

impl<I, O> OptionHandle<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// `OptionHandle<I, O> | OptionHandle<U, V> => OptionHandle<(I,U), (O,V)>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| Some(i + 1))
    ///             .and(Handle::new(1).spawn(move |i: i32| Some(i * 10))).handle((1, 1)).await;
    ///         assert_eq!(r, Some((2, 10)));
    ///     }
    /// )
    /// ```
    pub fn and<U, V>(self, rhs: OptionHandle<U, V>) -> OptionHandle<(I, U), (O, V)>
    where
        U: Send + 'static,
        V: Send + 'static,
    {
        self.join(rhs)
            .convert(|(o, v): (Option<O>, Option<V>)| match (o, v) {
                (Some(o), Some(v)) => Some((o, v)),
                _ => None,
            })
    }

    /// `OptionHandle<I, O> + OptionHandle<O, P> => OptionHandle<I, P>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| Some(i + 1))
    ///             .and_then(Handle::new(1).spawn(move |i: i32| Some(i * 10))).handle(1).await;
    ///         assert_eq!(r, Some(20));
    ///     }
    /// )
    /// ```
    pub fn and_then<P>(self, rhs: OptionHandle<O, P>) -> OptionHandle<I, P>
    where
        P: Send + 'static,
    {
        let ss = self;
        let rr = rhs;
        Handle::new(ss.n).spawn_tokio(Actor::from(move |i| {
            let ss = ss.clone();
            let rr = rr.clone();
            async move {
                let o = ss.handle(i).await;
                match o {
                    Some(o) => rr.handle(o).await,
                    None => None,
                }
            }
        }))
    }

    /// `OptionHandle<I, O> + Handle<O, P> => OptionHandle<I, P>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| Some(i + 1))
    ///             .map(Handle::new(1).spawn(move |i: i32| i * 10)).handle(1).await;
    ///         assert_eq!(r, Some(20));
    ///     }
    /// )
    /// ```
    pub fn map<P>(self, rhs: Handle<O, P>) -> OptionHandle<I, P>
    where
        P: Send + 'static,
    {
        let ss = self;
        let rr = rhs;
        Handle::new(ss.n).spawn_tokio(Actor::from(move |i| {
            let ss = ss.clone();
            let rr = rr.clone();
            async move {
                let o = ss.handle(i).await;
                match o {
                    Some(o) => Some(rr.handle(o).await),
                    None => None,
                }
            }
        }))
    }

    /// convert `OptionHandle` to `ResultHandle`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| Some(i + 1))
    ///             .ok_or("null error").handle(1).await;
    ///         assert_eq!(r.unwrap(), 2);
    ///     }
    /// )
    /// ```
    pub fn ok_or<C: ToString>(self, context: C) -> ResultHandle<I, O, anyhow::Error> {
        let context = context.to_string();
        self.convert(move |r: Option<O>| r.context(context.clone()))
    }
}

/// Handle return value is a `Result`
pub type ResultHandle<I, O, E> = Handle<I, Result<O, E>>;

/// Chain error together when connecto two `ResultHandle` together
#[derive(Debug, thiserror::Error)]
#[error("e={0}, en={1}")]
pub struct ChainError<E, EN>(E, EN);

unsafe impl<E, EN> Sync for ChainError<E, EN>
where
    E: Sync,
    EN: Sync,
{
}

impl<I, O, E> ResultHandle<I, O, E>
where
    I: Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
{
    /// `ResultHandle<I, O, E> + ResultHandle<U, V, EN> => ResultHandle<(I, U), (O, V), anyhow::Error>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// use thiserror;
    ///
    /// #[derive(Debug, thiserror::Error)]
    /// pub enum Error{}
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| {
    ///                 Result::<i32, Error>::Ok(i+1)
    ///              })
    ///             .and(
    ///                 Handle::new(1).spawn(move |i: i32| {
    ///                 Result::<i32, Error>::Ok(i*1)
    ///             })
    ///         ).handle((1, 1)).await;
    ///         assert_eq!(r.unwrap(), (2, 1));
    ///     }
    /// );
    /// ```
    pub fn and<U, V, EN>(
        self,
        rhs: ResultHandle<U, V, EN>,
    ) -> ResultHandle<(I, U), (O, V), anyhow::Error>
    where
        E: Sync + std::error::Error,
        U: Send + 'static,
        V: Send + 'static,
        EN: Sync + Send + std::error::Error + 'static,
    {
        self.join(rhs)
            .convert(|(o, v): (Result<O, E>, Result<V, EN>)| match (o, v) {
                (Ok(o), Ok(v)) => Ok((o, v)),
                (Err(e), Err(en)) => Err(anyhow::Error::new(ChainError(e, en))),
                (Err(e), _) => Err(anyhow::Error::new(e)),
                (_, Err(e)) => Err(anyhow::Error::new(e)),
            })
    }

    /// `ResultHandle<I, O, E> | ResultHandle<O, P, EN> => ResultHandle<I, P, anyhow::Error>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// use thiserror;
    ///
    /// #[derive(Debug, thiserror::Error)]
    /// pub enum Error{}
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| {
    ///                 Result::<i32, Error>::Ok(i+1)
    ///              })
    ///             .and_then(
    ///                 Handle::new(1).spawn(move |i: i32| {
    ///                 Result::<i32, Error>::Ok(i*10)
    ///             })
    ///         ).handle(1).await;
    ///         assert_eq!(r.unwrap(), 20);
    ///     }
    /// );
    /// ```
    pub fn and_then<P, EN>(self, rhs: ResultHandle<O, P, EN>) -> ResultHandle<I, P, anyhow::Error>
    where
        E: Sync + std::error::Error,
        P: Send + 'static,
        EN: Sync + Send + std::error::Error + 'static,
    {
        let ss = self;
        let rr = rhs;
        Handle::new(ss.n).spawn_tokio(Actor::from(move |i| {
            let ss = ss.clone();
            let rr = rr.clone();
            async move {
                let o = ss.handle(i).await?;
                let p = rr.handle(o).await?;
                anyhow::Result::Ok(p)
            }
        }))
    }

    /// `ResultHandle<I, O, E> | Handle<O, P> => ResultHandle<I, P, E>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// use thiserror;
    ///
    /// #[derive(Debug, thiserror::Error)]
    /// pub enum Error{}
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| {
    ///                 Result::<i32, Error>::Ok(i+1)
    ///              })
    ///             .map(
    ///                 Handle::new(1).spawn(move |i: i32| {
    ///                 i*10
    ///             })
    ///         ).handle(1).await;
    ///         assert_eq!(r.unwrap(), 20);
    ///     }
    /// );
    /// ```
    pub fn map<P>(self, rhs: Handle<O, P>) -> ResultHandle<I, P, E>
    where
        P: Send + 'static,
    {
        let ss = self;
        let rr = rhs;
        Handle::new(ss.n).spawn_tokio(Actor::from(move |i| {
            let ss = ss.clone();
            let rr = rr.clone();
            async move {
                let o = ss.handle(i).await?;
                let p = rr.handle(o).await;
                anyhow::Result::Ok(p)
            }
        }))
    }

    /// `ResultHandle<I, O, E> => OptionHandle<I, O>`
    /// ```rust
    /// use tokiactor::*;
    /// use tokio;
    /// use thiserror;
    ///
    /// #[derive(Debug, thiserror::Error)]
    /// pub enum Error{}
    ///
    /// tokio::runtime::Runtime::new().unwrap().block_on(
    ///     async move {
    ///         let r = Handle::new(1)
    ///             .spawn(move |i: i32| {
    ///                 Result::<i32, Error>::Ok(i+1)
    ///              }).ok().handle(1).await;
    ///         assert_eq!(r.unwrap(), 2);
    ///     }
    /// );
    /// ```
    pub fn ok(self) -> OptionHandle<I, O> {
        self.convert(move |r: Result<O, E>| r.ok())
    }
}
