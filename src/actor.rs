use async_trait::async_trait;
use itertools::Itertools;
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{
    handle::Handle,
    registry::{ActorSocket, GlobalRegisty},
};

pub trait Actor<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
    Self: Sized + Send + 'static,
{
    fn handle(&mut self, input: I) -> O;

    fn spawn(self, cap: usize) -> Handle<I, O> {
        let ActorSocket { tx: handle, rx } = GlobalRegisty::<Self, I, O>::get_or_set(cap);
        let mut ss = self;
        std::thread::spawn(move || {
            while let Ok((i, tx)) = rx.recv_blocking() {
                let o = ss.handle(i);
                if tx.send(o).is_err() {
                    return;
                }
            }
        });
        Handle { handle }
    }

    fn spawn_n(self, n: usize, cap: usize) -> Handle<I, O>
    where
        Self: Clone,
    {
        assert!(n > 0);
        for i in 0..n {
            let handle = self.clone().spawn(cap);
            if i + 1 == n {
                return handle;
            }
        }
        unreachable!("not here")
    }

    fn n_spawn<IT>(it: IT, cap: usize) -> Handle<I, O>
    where
        IT: IntoIterator<Item = Self>,
    {
        let actors = it.into_iter().collect_vec();
        let actor_n = actors.len();
        for (n, actor) in actors.into_iter().enumerate() {
            let handle = actor.spawn(cap);
            if n + 1 == actor_n {
                return handle;
            }
        }
        unreachable!("not here")
    }
}
#[derive(Clone)]
pub struct ActorFn<I, O, FN> {
    f: FN,
    _i: std::marker::PhantomData<I>,
    _o: std::marker::PhantomData<O>,
}

impl<I, O, FN> Actor<I, O> for ActorFn<I, O, FN>
where
    I: Send + 'static,
    O: Send + 'static,
    FN: FnMut(I) -> O + Send + 'static,
{
    fn handle(&mut self, input: I) -> O {
        (self.f)(input)
    }
}

impl<I, O, FN> ActorFn<I, O, FN>
where
    I: Send + 'static,
    O: Send + 'static,
    FN: FnMut(I) -> O + Send + 'static,
{
    pub fn new(f: FN) -> Self {
        Self {
            f,
            _i: std::marker::PhantomData,
            _o: std::marker::PhantomData,
        }
    }
}

#[async_trait]
pub trait ActorFuture<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
    Self: Sized + Send + Clone + 'static,
{
    async fn handle(&mut self, input: I) -> O;

    fn spawn(self, cap: usize) -> Handle<I, O> {
        let ActorSocket { tx: handle, rx } = GlobalRegisty::<Self, I, O>::get_or_set(cap);
        let ss = self;
        tokio::spawn(async move {
            let error = Arc::from(AtomicBool::new(false));
            while let Ok((i, tx)) = rx.recv().await {
                if error.load(Ordering::SeqCst) {
                    break;
                }
                let error = error.clone();
                let mut ss = ss.clone();
                tokio::spawn(async move {
                    let o = ss.handle(i).await;
                    if tx.send(o).is_err() {
                        error.store(true, Ordering::SeqCst);
                    }
                });
            }
        });
        Handle { handle }
    }

    fn spawn_n(self, n: usize, cap: usize) -> Handle<I, O>
    where
        Self: Clone,
    {
        assert!(n > 0);
        for i in 0..n {
            let handle = self.clone().spawn(cap);
            if i + 1 == n {
                return handle;
            }
        }
        unreachable!("not here")
    }

    fn n_spawn<IT>(it: IT, cap: usize) -> Handle<I, O>
    where
        IT: IntoIterator<Item = Self>,
    {
        let actors = it.into_iter().collect_vec();
        let actor_n = actors.len();
        for (n, actor) in actors.into_iter().enumerate() {
            let handle = actor.spawn(cap);
            if n + 1 == actor_n {
                return handle;
            }
        }
        unreachable!("not here")
    }
}

pub struct ActorFutureFn<I, O, F, FN>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Future<Output = O> + Send + 'static,
    FN: Fn(I) -> F + Send + 'static + Clone,
{
    f: FN,
    _i: std::marker::PhantomData<I>,
    _o: std::marker::PhantomData<O>,
}

impl<I, O, F, FN> Clone for ActorFutureFn<I, O, F, FN>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Future<Output = O> + Send + 'static,
    FN: Fn(I) -> F + Send + 'static + Clone,
{
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            _i: self._i.clone(),
            _o: self._o.clone(),
        }
    }
}

#[async_trait]
impl<I, O, F, FN> ActorFuture<I, O> for ActorFutureFn<I, O, F, FN>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Future<Output = O> + Send + 'static,
    FN: Fn(I) -> F + Send + 'static + Clone,
{
    async fn handle(&mut self, i: I) -> O {
        (self.f)(i).await
    }
}

impl<I, O, F, FN> ActorFutureFn<I, O, F, FN>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Future<Output = O> + Send + 'static,
    FN: Fn(I) -> F + Send + 'static + Clone,
{
    pub fn new(f: FN) -> Self {
        Self {
            f,
            _i: std::marker::PhantomData,
            _o: std::marker::PhantomData,
        }
    }
}
