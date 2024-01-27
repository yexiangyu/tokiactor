use async_channel as channel;
use once_cell::sync::Lazy;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    marker::PhantomData,
    sync::{Arc, Mutex},
};
use tokio::sync::oneshot;

struct HandleEntry {
    cap: usize,
    sock: Box<dyn Any>,
}

unsafe impl Send for HandleEntry {}

static GLOBAL_HANDLE_REGISTRY: Lazy<Arc<Mutex<HashMap<TypeId, HandleEntry>>>> =
    Lazy::new(|| Arc::from(Mutex::from(HashMap::new())));

pub struct ActorSocket<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub tx: channel::Sender<(I, oneshot::Sender<O>)>,
    pub rx: channel::Receiver<(I, oneshot::Sender<O>)>,
}

impl<I, O> Clone for ActorSocket<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            rx: self.rx.clone(),
        }
    }
}

pub struct GlobalRegisty<T, I, O> {
    _t: PhantomData<T>,
    _i: PhantomData<I>,
    _o: PhantomData<O>,
}

#[allow(unused)]
impl<T, I, O> GlobalRegisty<T, I, O>
where
    T: Sized + Send + 'static,
    I: Send + 'static,
    O: Send + 'static,
{
    pub fn get() -> Option<ActorSocket<I, O>> {
        let lck = GLOBAL_HANDLE_REGISTRY
            .lock()
            .expect("failed to lock global handle registry");
        lck.get(&TypeId::of::<T>())
            .and_then(|entry| entry.sock.downcast_ref::<ActorSocket<I, O>>())
            .cloned()
    }

    pub fn set(cap: usize) -> ActorSocket<I, O> {
        let mut lck = GLOBAL_HANDLE_REGISTRY
            .lock()
            .expect("failed to lock global handle registry");
        let (tx, rx) = match cap {
            0 => channel::unbounded::<(I, oneshot::Sender<O>)>(),
            _ => channel::bounded::<(I, oneshot::Sender<O>)>(cap),
        };
        let socket = ActorSocket { tx, rx };
        lck.insert(
            TypeId::of::<T>(),
            HandleEntry {
                cap,
                sock: Box::new(socket.clone()),
            },
        );
        socket
    }

    pub fn get_or_set(cap: usize) -> ActorSocket<I, O> {
        let mut lck = GLOBAL_HANDLE_REGISTRY
            .lock()
            .expect("failed to lock global handle registry");

        let typ = TypeId::of::<T>();

        let create_entry = || {
            let (tx, rx) = match cap {
                0 => channel::unbounded::<(I, oneshot::Sender<O>)>(),
                _ => channel::bounded::<(I, oneshot::Sender<O>)>(cap),
            };
            HandleEntry {
                cap,
                sock: Box::new(ActorSocket { tx, rx }),
            }
        };

        let entry = lck
            .entry(typ)
            .and_modify(|entry| {
                if entry.cap != cap {
                    *entry = create_entry();
                }
            })
            .or_insert_with(create_entry);

        entry
            .sock
            .downcast_ref::<ActorSocket<I, O>>()
            .expect("failed to get actor socket")
            .clone()
    }

    pub fn delete(&self) {
        let mut lck = GLOBAL_HANDLE_REGISTRY
            .lock()
            .expect("failed to lock global handle registry");
        lck.remove(&TypeId::of::<T>());
    }
}
