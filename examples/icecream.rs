use futures::StreamExt;
use std::time::Instant;

use rand::*;
use tokiactor::*;
use tracing::*;

#[derive(Debug, thiserror::Error)]
pub enum Flavor {
    #[error("chocox{0}")]
    Choco(u32),
    #[error("berryx{0}")]
    Berry(u32),
    #[error("plain")]
    Plain,
}

#[derive(Debug, thiserror::Error)]
#[error("icecream milk={}+sugar={}+{}", .milk, .sugar, .flavor)]
struct Icecream {
    milk: u32,
    sugar: u32,
    flavor: Flavor,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let milk = init_store("milk");
    let sugar = init_store("sugar");
    let choco = init_store("choco");
    let berry = init_store("berry");

    let pipe = milk
        .and(sugar)
        .and(Handle::new(10).spawn_tokio(move |f: Flavor| {
            let berry = berry.clone();
            let choco = choco.clone();
            async move {
                match f {
                    Flavor::Berry(needs) => berry.handle(needs).await.map(|res| Flavor::Berry(res)),
                    Flavor::Choco(needs) => choco.handle(needs).await.map(|res| Flavor::Choco(res)),
                    Flavor::Plain => Some(Flavor::Plain),
                }
            }
        }))
        .convert(|i| {
            i.map(|((milk, sugar), flavor)| Icecream {
                milk,
                sugar,
                flavor,
            })
        });

    let factory = Handle::new(10)
        .spawn(move |i: Icecream| {
            let Icecream {
                milk,
                sugar,
                flavor,
            } = i;
            ((milk, sugar), flavor)
        })
        .then(pipe);

    let orders = (0..20).map(|_| generate_order());

    let tm = Instant::now();
    for order in orders {
        let tm = Instant::now();
        let i = factory.handle(order).await;
        match i {
            Some(i) => info!("[factory] return {}, delta={:?}", i, tm.elapsed()),
            None => info!("[factory] return none, delta={:?}", tm.elapsed()),
        }
    }
    info!("[factory] process 10 orders in {:?}", tm.elapsed());

    let orders = (0..20).map(|_| generate_order());
    let tm = Instant::now();
    let _ = futures::stream::iter(orders)
        .map(|o| async {
            let tm = Instant::now();
            let i = factory.handle(o).await;
            match i {
                Some(i) => info!("[factory] return {}, delta={:?}", i, tm.elapsed()),
                None => info!("[factory] return none, delta={:?}", tm.elapsed()),
            }
        })
        .buffered(10)
        .collect::<Vec<_>>()
        .await;
    info!(
        "[factory] process 10 orders in {:?} in parallel",
        tm.elapsed()
    );
}

fn init_store(name: &'static str) -> Handle<u32, Option<u32>> {
    let mut n_available = rand::thread_rng().gen_range(8u32..20);
    info!("[{name}] init store with {n_available} items");
    Handle::new(10).spawn_n(
        10,
        Actor::from(move |needs: u32| {
            let ms = rand::thread_rng().gen_range(0..500);
            std::thread::sleep(std::time::Duration::from_millis(ms));
            let r = match needs > n_available {
                true => {
                    let rfill = rand::thread_rng().gen_range(8u32..20);
                    n_available += rfill;
                    error!("[{name}] out of store, refill {rfill} items in {ms}ms");
                    None
                }
                false => {
                    n_available -= needs;
                    debug!("[{name}] return {needs} items in {ms}ms");
                    Some(needs)
                }
            };
            r
        }),
    )
}

fn generate_order() -> Icecream {
    let milk = rand::thread_rng().gen_range(1..3);
    let sugar = rand::thread_rng().gen_range(1..3);
    let flavor = match rand::thread_rng().gen::<u32>() % 3 {
        0 => Flavor::Berry(rand::thread_rng().gen_range(1..3)),
        1 => Flavor::Choco(rand::thread_rng().gen_range(1..3)),
        _ => Flavor::Plain,
    };
    Icecream {
        milk,
        sugar,
        flavor,
    }
}
