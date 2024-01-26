#![doc = include_str!("../README.md")]

pub mod actor;
pub mod error;
pub mod handle;
pub(crate) mod registry;

pub use anyhow;
pub use futures;
pub use tokio;

pub mod prelude {
    pub use super::actor::{Actor, ActorFn, ActorFuture, ActorFutureFn};
    pub use super::handle::{Handle, OptionHandle, ResultHandle};
}
