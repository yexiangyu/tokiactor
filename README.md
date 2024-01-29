# [Tokiactor](https://github.com/yexiangyu/tokiactor)

## About [Tokiactor](https://github.com/yexiangyu/tokiactor)

[tokiactor](https://github.com/yexiangyu/tokiactor) is a minimized implementation of `actor` pattern based on `tokio` runtime. No concepts like ~~`System`~~ or ~~`Context`~~ or ~~`Message`~~ involved, just `Handle` and `Actor`.

In `tokiactor`, `Actor` is a wrapper of `sync` and `async` function. `sync` function will be executed in multiple system threads, `async` function will be executed in `tokio` green thread asynchronously.

Large batch tasks like processing thousands of pictures can be done in parallel by leveraging buffered `futures::StreamExt` trait from crate `futures`.

## Installation

Add `tokiactor` to `Cargo.toml`, `tokiactor` needs `tokio` to make things work.

```toml
[dependencies]
tokiactor = "*"
```

## Getting start

Following code will create `Adder` actor, then, actor spawned in `Handle`, `Adder` will be called thru `Handle::handle` method asynchronously.

```rust
use tokio;
use tokiactor::*;

let rt = tokio::runtime::Runtime::new().unwrap().block_on(
	async move {
		let handle = Handle::new(1).spawn(move |i: i32| i+ 41);
		assert_eq!(handle.handle(1).await, 42);
	}
);
```
or, we can create `Actor` from `async` `Closure`:

```rust
use tokio;
use tokiactor::*;

let rt = tokio::runtime::Runtime::new().unwrap().block_on(
	async move {
		let handle = Handle::new(1).spawn_tokio(move |i: i32| async move {i + 41});
		assert_eq!(handle.handle(1).await, 42);
	}
);
```

or, create `Actor` from blocking `fn`, then run `Actor` in `parallel`

```rust
use tokio;
use tokiactor::*;
use futures::StreamExt;

fn adder_fn(i: i32) -> i32 {
	std::thread::sleep(std::time::Duration::from_secs(1));
	i+ 41
}

let rt = tokio::runtime::Runtime::new().unwrap().block_on(
	async move {
		let handle = Handle::new(10).spawn_n(10, adder_fn);
		let results = futures::stream::iter((0..10))
			.map(|i| handle.handle(i))
			.buffered(10)
			.collect::<Vec<_>>().await;
		assert_eq!(results[9], 50)
	}
);
```

###  Actor spawn

There are many different ways to spawn an `Actor`:

-  To spawn ***sync*** `Actor`

	- `Handle::spawn`: spawn `Actor` in `1` background thread

	- `Handle::spawn_n`: spawn `n` `Actor` in `fn` impl `Clone` in `n` background threads.

-  To spawn ***async*** `Actor`

	- `Handle::spawn_tokio`: spawn `Actor` in background tokio thread, all task will be executed in independent specific tokio thread.

please check [docs.rs](http://docs.rs/tokiactor) for further infomation.

### Handle<I, O>

`Handle<I, O>` can be connected together, build another type of `Handle`

```rust
use tokio;
use tokiactor::*;

let rt = tokio::runtime::Runtime::new().unwrap().block_on(
	async move {
		let add = Handle::new(1).spawn(move |i: i32| i + 1);
		let sub = Handle::new(1).spawn(move |i: i32| i - 1);
		let div = Handle::new(1).spawn(move |i: i32| {
			match i == 0 {
				false => Some(10/i),
				true => None
			}
		});
		let mul = Handle::new(1).spawn(move |i: i32| i * 10);

		let handle = add.then(sub).then(div).map(mul);

		assert_eq!(handle.handle(0).await, None);
		assert_eq!(handle.handle(2).await, Some(50));
	}
);
```
Please check `examples/icecream.rs` out for more complicated use case.
