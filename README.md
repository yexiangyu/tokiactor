# [Tokiactor](https://github.com/yexiangyu/tokiactor)

## About [Tokiactor](https://github.com/yexiangyu/tokiactor)

[tokiactor](https://github.com/yexiangyu/tokiactor) is a minimized implementation of `actor` pattern based on `tokio` runtime. No concepts like `System` or `Context` or `Message`, just `Handle`, `Actor` and `ActorFuture`.

Both `sync` and `async` actor are supported. `sync` function can be executed on multiple system threads, then be called by `Handle::handle` method asynchronously.

Large batch tasks like processing thousands of pictures can be done in parallel by leveraging buffered `futures::Stream` trait from crate `futures`.

## Installation

Add `tokiactor` to `Cargo.toml`, `tokiactor` need `tokio` to make things work.

```toml
[dependencies]
tokiactor = "*"
```

## Getting started

### Create actor

Let's start from create `Actor` first. Actor can be implemented by implying `Actor` or `ActorFuture` trait: 
```rust
use tokiactor::prelude::*;

struct SlowAdder;

impl Actor<i32, i32> for SlowAdder
{
	fn handle(&mut self, i: i32) -> i32
	{
		std::thread::sleep(std::time::Duration::from_secs(1));
		i + 1
	}
}
```
or, create `Actor` from `fn`:
```rust
use tokiactor::prelude::*;

fn slow_adder_fn_impl(i: i32) -> i32 {
	std::thread::sleep(std::time::Duration::from_secs(1));
	i + 1
}

let slow_adder_actor = ActorFn::new(slow_adder_fn_impl);
```
or, create `Actor` from `Closure`:
```rust
use tokiactor::prelude::*;

let slow_adder_actor = ActorFn::new(|i: i32| { 
	std::thread::sleep(std::time::Duration::from_secs(1));
	i + 1
});
```
or, create `Actor` from `async fn` or `Closure` return `Future` type:
```rust
use tokiactor::{prelude::*, tokio};

async fn slow_adder_fn_impl(i: i32) -> i32 {
	tokio::time::sleep(std::time::Duration::from_secs(1)).await;
	i + 1
}

let slow_adder_actor = ActorFutureFn::new(slow_adder_fn_impl);

let another_slow_adder_actor = ActorFutureFn::new(|i: i32| { 
	async move {
		tokio::time::sleep(std::time::Duration::from_secs(1)).await;
		i + 1
	}
});
```

### Spawn then run with `Handle`

Every `Actor` need to `spawn` to run in background, `spawn` will return `Handle`, then `Actor` can handle request thru `Handle::handle` method asynchronously. `spawn` ***must*** be called in `tokio` runtime.
```rust
use tokiactor::{tokio, prelude::*};
let rt = tokio::runtime::Runtime::new().unwrap();
rt.block_on(
	async move {
		let adder_actor = ActorFutureFn::new(|i: i32| { 
			async move {
				tokio::time::sleep(std::time::Duration::from_secs(1)).await;
				i + 1
			}
		});
		let handle = adder_actor.spawn(1); // spawn actor, with channel depth = 1
		assert_eq!(handle.handle(1).await, 2);
});
```

If `Actor` impl `Clone` trait, `spawn_n` can help to spawn multiple independent `Actor`, to loadbalance the request: 

```rust
use tokiactor::{prelude::*, tokio, futures};

// this function is clonable
fn slow_adder_fn_impl(i: i32) -> i32 {
	std::thread::sleep(std::time::Duration::from_secs(1));
	i + 1
}
let adder_actor = ActorFn::new(slow_adder_fn_impl);
let rt = tokio::runtime::Runtime::new().unwrap();
rt.block_on(
	async move {
		let add_handle = adder_actor.spawn_n(10, 1);
		let mut results = vec![];
		for i in 0..10
		{
			let add_handle = add_handle.clone();
			let result = tokio::spawn(async move {
				add_handle.handle(i).await
			});
			results.push(result);
		}
		for (n, result) in results.into_iter().enumerate()
		{
			assert_eq!(n as i32 + 1, result.await.unwrap());
		}
	}
);
```
The code above did use all `Actor` spawned, but with unnecessary steps like clone `Handle` and `tokio::spawn`, or, we can use `futures::StreamExt` and `futures::Stream` to make actors work in parallel more gracefully:
```rust
use tokiactor::{prelude::*, tokio, futures};
use futures::{Stream, StreamExt};

// this function is clonable
fn slow_adder_fn_impl(i: i32) -> i32 {
	std::thread::sleep(std::time::Duration::from_secs(1));
	i + 1
}
let adder_actor = ActorFn::new(slow_adder_fn_impl);
let rt = tokio::runtime::Runtime::new().unwrap();
rt.block_on(
	async move {
		let add_handle = adder_actor.spawn_n(10, 1);

		let results = futures::stream::iter((0..10))
		    .map(|i| add_handle.handle(i))
			.buffered(10)
			.collect::<Vec<_>>().await;

		for (n, result) in results.into_iter().enumerate()
		{
			assert_eq!(n as i32 + 1, result);
		}
	}
);
```
### Connect `Handle` together

`Handle` implement some helper function to chain different `Handle` with each other, like `then`, `join`, `and_then`..., here is an example: 
```rust
use tokiactor::{prelude::*, tokio, futures};
use futures::{Stream, StreamExt};

let rt = tokio::runtime::Runtime::new().unwrap();
rt.block_on(
	async move {

		let adder = ActorFn::new(|i: i32| { i + 1 }).spawn(1);
		let f32er = ActorFn::new(|i: i32| { i as f32}).spawn(1);
		let muler = ActorFn::new(|i: f32| { i * 2.0 }).spawn(1);

		// connect adder, f32er, muler together.
		let handle = adder.then(f32er).then(muler);
		assert!((handle.handle(1).await - 4.0).abs() < 0.0000001);

		// divder will return None if input equals 0.0
		let diver = ActorFn::new(|i: f32| { match i == 0.0 {
			true => None,
			false => Some(10.0/i)
		} }).spawn(1);

		let abser = ActorFn::new(|i: f32| { i.abs() }).spawn(1);

		// connect handler with diver, then map to abser
		let handle = handle.then(diver).map(abser);

		assert!(handle.handle(-1).await.is_none());

		assert!(handle.handle(1).await.is_some());
	}
)
```

## What's next in the future version of `tokiactor`

Here is a possible `TODO` list in mind: 

- [] `Router`: `Actor` is isomorphic now, which means request to `Handle`  could not be routed to specific `Actor` instance. 
- [] `Autoscale`: `Actor` now spawn manually with some specific instances number, what about autoscaling more `Actor` if needed?
