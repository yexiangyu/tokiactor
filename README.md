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

## Getting start

Following code will create `Adder` struct, which will impl `Actor`, then `Adder::spawn()` return a `handle`, then call `Adder` throught `Handle`:

```rust
use tokiactor::actor::Actor;

// define an actor
struct Adder
{
	pub to_add: i32
}

// impl Actor trait
impl Actor<i32, i32> for Adder
{
	fn handle(&mut self, i: i32) -> i32
	{
		let r= self.to_add + i;
		self.to_add += i;
		r
	}
}

// tokio runtime is needed
let rt = tokio::runtime::Runtime::new().unwrap();

// spawn actor, return handle, handle do task
rt.block_on(async move {

	// create instance of Adder, then spawn it
	let adder_handle = Adder { to_add: 41}.spawn(1);

	// do the calculation asynchronously, adder's status is also change.
	let r = adder_handle.handle(1).await;

	assert_eq!(r, 42);
});
```

## Actor

There are 2 kinds of `Actor` traits in `tokiactor`, Actor can be implemented by implying `Actor` or `ActorFuture` trait, `tokiactor` provide different way to create them: 

- `Actor`: sync function wrapper, executed in `std::thread::Thread` in background.

	- create actor from `struct`

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
	- create actor from `Closure`

	```rust
	use tokiactor::prelude::*;
	let adder = ActorFn::new(|n: i32| n + 1);
	```

	- create actor from `fn`
	```rust
	use tokiactor::prelude::*;
	fn adder(i: i32) -> i32 {i + 1i32}
	let adder = ActorFn::new(adder);
	```


- `ActorFuture`: async function wrapper, executed in tokio green thread, [async-trait](https://docs.rs/async-trait/latest/async_trait/) is needed to impl `ActorFuture`.
	- create from `struct`

	```rust
	use tokiactor::prelude::*;
	use async_trait::async_trait;

	#[derive(Clone)] // TODO: this 'Clone' derive should be eleminated in the future?;
	struct Adder;

	#[async_trait]
	impl ActorFuture<i32, i32> for Adder
	{
		async fn handle(&mut self, i: i32) -> i32
		{
			i + 1
		}
	}
	```

	- create from `Closure` return `Future`

	```rust
	use tokiactor::prelude::*;
	let adder = ActorFutureFn::new(|i: i32| async move { i + 1});
	```

	- create from `async fn`

	```rust
	use tokiactor::prelude::*;
	async fn add(i: i32) -> i32
	{
		i + 1
	}
	let adder = ActorFutureFn::new(add);
	```

## spawn

 Both `Actor` and `ActorFuture` impl different spawn methods, the spawn will return `Handle`:

 - `spawn(self, cap: usize)`

   	spawn single instance of actor, `cap` is the deepth of channel for `Actor`.

	```rust
	use tokiactor::prelude::*;
	let adder = ActorFn::new(|i: i32| i + 1);
	let handle: Handle<i32, i32> = adder.spawn(1);
	```

 - `spawn_n(self, n: usize, cap: usize)`
	
	spawn `n` instances of actor, `cap` is the deepth of channel for `Actor`, if `Actor` impl `Clone` trait.

	```rust
	use tokiactor::prelude::*;
	let adder = ActorFn::new(|i: i32| i + 1);
	let handle: Handle<i32, i32> = adder.spawn_n(10, 1); //spwan 10 instance of adder
	```
- `n_spawn(actors: IntoIterator<Item = Actor>, cap: usize)`

  spawn many instances of actor, from iterator of `Actor`, `cap` is the deepth of channel for `Actor`
  ```rust
  use tokiactor::prelude::*;
  use futures::Future;
  use async_trait::async_trait;

  #[derive(Clone)]
  struct Adder;

  impl Actor<i32, i32> for Adder
  {

	fn handle(&mut self, i: i32) -> i32 
	{
		  i + 1
	}
  }

  let rt = tokio::runtime::Runtime::new().unwrap();

  rt.block_on(async move {
	let handle = Adder::n_spawn([Adder, Adder], 1);
	assert_eq!(handle.handle(1).await, 2);
  });
  ```

### Handle

Different ```Handle``` can be connected to build new type of `Handle`.

```rust
use tokiactor::prelude::*;
use tokio;

let add = ActorFutureFn::new(|i: i32| async move { i + 1 });
let min = ActorFutureFn::new(|i: i32| async move { i - 1 });
let mul = ActorFn::new(|i: i32| i * 42);
let div = ActorFn::new(|i: i32| match i== 0 {true => None, false => Some(100.0/i as f32)});

let rt = tokio::runtime::Runtime::new().unwrap();

rt.block_on(
	async move {
		// spawn 4 actor to handle
		let add = add.spawn(1);
		let min = min.spawn(1);
		let mul = mul.spawn(1);
		let div = div.spawn(1);

		// connect all handle together
		// 100/((i + 1)  - 1) * 42
		let all = add.then(min).then(div).convert(|i: Option<f32>| i.map(|i| i as i32)).map(mul); 

		assert_eq!(all.handle(0).await, None);
	}
)
```


## What's next in the future version of `tokiactor`

Here is a possible `TODO` list in mind: 

- [] `Router`: `Actor` is isomorphic now, which means request to `Handle`  could not be routed to specific `Actor` instance. 
- [] `Autoscale`: `Actor` now spawn manually with some specific instances number, what about autoscaling more `Actor` if needed?
- [] remove `Clone` for `ActorFuture`
