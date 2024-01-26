# [Tokiactor](https://github.com/yexiangyu/tokiactor)

## About [Tokiactor](https://github.com/yexiangyu/tokiactor)

[tokiactor](https://github.com/yexiangyu/tokiactor) is a minimized workable crate to implement `actor` pattern. [tokiactor](https://github.com/yexiangyu/tokiactor) can provide high level abstraction of function call, integrate diffenent function into pipeline then run. [tokiactor](https://github.com/yexiangyu/tokiactor) provides capabilies of: 

- Minimun implemention of high level abstraction of `Actor`, NO `Context`, `Message`..., but only `I` and `O`
- ***Parallel*** execution for time consuming blocking function.
- `sync` and `async` function bridge and integration with `Handle<I, O>`. 
- Build simple compute `graph` by connecting `Handle<I, O>`s together.

## Quick Start

```rust
// let's make a icecream factory
use tracing::*;
use tokiactor::prelude::*;

// icecream is made by milk and sugar
struct Icecream
{
	milk: i32,
	sugar: i32
}

// customer order a icecream with some milk and sugar
struct Order(Icecream);

// things won't work everytime, error might happened
#[derive(Debug, thiserror::Error)]
pub enum Error
{
	#[error("not enough milk")]
	NotEnoughMilk
}

// milk tank with some milk
struct MilkTank {
	pub milks: i32
}

impl Actor<i32, Result<i32, Error>> for MilkTank
{
	fn handle(&mut self, needs: i32) -> Result<i32, Error>
	{
		if needs > self.milks
		{
			self.milks += 10;
			error!("running out of milk, may be next time");
			return Err(Error::NotEnoughMilk);
		}
		info!(?needs, "we got lot's of milk, return");
		self.milks -= needs;
		Ok(needs)
	}
}


tokio::runtime::Runtime::new().unwrap().block_on(
	async move {
		let _ = tracing_subscriber::fmt::try_init();
		// create waiter actor with async closure and spawn it
		let waiter_handle = ActorFn::new(move |Order(Icecream{milk, sugar}): Order|  {
			info!(%milk, %sugar, "recv order");
			(milk, sugar)
		}).spawn(1);

		// create sugar actor with async closure and spawn it
		let sugar_handle = ActorFutureFn::new(move |needs: i32| async move {
			info!("we have ulimited sugur available, return sugur: {}", needs);
			needs
		}).spawn(1);

		// create milk actor and spawn it
		let milk_handle = MilkTank {milks: 10}.spawn(1);

		// now we need to freeze ingredient for 10 seconds
		let freeze_handle = ActorFutureFn::new(move |(sugar, milk): (i32, Result<i32, Error>) | async move {
			milk.map(|milk| Icecream{ milk, sugar})
		}).spawn(1);

		// Assemble things together
		let pipeline = waiter_handle.then(sugar_handle.join(milk_handle)).then(freeze_handle);
		for i in 0..10
		{
			let icecream = pipeline.handle(Order(Icecream{milk: 1, sugar: 1})).await;
			assert!(icecream.is_ok());
		}
		let icecream = pipeline.handle(Order(Icecream{milk: 1, sugar: 1})).await;
		assert!(icecream.is_err());
	}
);
```

## Details

There are 3 major components in [tokiactor](https://github.com/yexiangyu/tokiactor):

### `trait Actor<I, O>`
By implementing `trait Actor<I, O>`, `struct` or `closure` or `fn` will be wrapped in system `thread`, accpet `I` and return `O` asynchronously.

### `trait ActorFuture<I, O>`
by implementing `trait ActorFuture<I, O>`, `struct` or `closure` or `fn` will be wrapped in [tokio](https://tokio.rs/) thread, accpet `I` and return `O` asynchronously.

### `struct Handle<I, O>`: 
`struct Handle<I, O>` create an async function interface: `handle` to talk with both `Actor` or `ActorFuture`

![tokio](https://tokio.rs/img/tokio-horizontal.svg "Tokio")

[tokio](https://tokio.rs) is used to drive as async runtime.