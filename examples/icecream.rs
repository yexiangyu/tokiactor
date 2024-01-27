use rand::Rng;
use std::sync::Arc;
use std::time::Duration;
use tokiactor::prelude::*;
use tokio::sync::Mutex;
use tracing::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("started icecream factory");

    let milk = create_store("milk").await;
    let sugar = create_store("sugar").await;

    let base = milk.and(sugar);

    let berry = create_store("berry").await;
    let choco = create_store("choco").await;

    let flavor = berry.and(choco);

    let prepare = base.and(flavor);

    let prepare = ActorFutureFn::new(move |(m, s, b, c): (u32, u32, u32, u32)| {
        let prepare = prepare.clone();
        async move {
            match prepare.handle(((m, s), (b, c))).await {
                Some(((m, s), (b, c))) => Some((m, s, b, c)),
                None => None,
            }
        }
    })
    .spawn(1);

    for _ in 0..20 {
        loop {
            let (m, s, b, c) = generate_order();
            match prepare.handle((m, s, b, c)).await {
                Some((m, s, b, c)) => {
                    info!("got icecream with milk={m}, sugar={s}, berry={b}, choco={c}");
                    break;
                }
                None => {
                    error!("fail icecream with milk={m}, sugar={s}, berry={b}, choco={c}");
                }
            }
        }
    }
}

fn generate_order() -> (u32, u32, u32, u32) //(milk, sugar, berry, choco)
{
    let milk = rand_u32(1, 3);
    let sugar = rand_u32(0, 2);
    let (berry, choco) = match rand_u32(0, 3) {
        0 => (0, rand_u32(1, 2)),              //choco only
        1 => (rand_u32(1, 2), 0),              // berry only
        2 => (rand_u32(1, 2), rand_u32(1, 2)), // berry and choco
        _ => (0, 0),                           //none
    };
    (milk, sugar, berry, choco)
}

async fn create_store(name: &'static str) -> Handle<u32, Option<u32>> {
    let n = Arc::from(Mutex::from(rand_u32(1, 10)));
    info!(
        "[store][{}] initialized with {} items in store",
        name,
        n.lock().await,
    );
    ActorFutureFn::new(move |needs: u32| {
        let n = n.clone();
        async move {
            let mut nn = n.lock().await;
            let dur = async_sleep().await;
            match needs > *nn {
                true => {
                    error!("[store][{}] out of store, delta={:?}", name, dur);
                    let n = n.clone();
                    tokio::spawn(async move {
                        let refill = rand_u32(1, 100);
                        let dur = async_sleep().await;
                        let mut n = n.lock().await;
                        *n += refill;
                        info!("[store][{}] refill {} items, delta={:?}", name, *n, dur);
                    });
                    None
                }
                false => {
                    *nn -= needs;
                    debug!("[store][{}] provide {} items, delta={:?}", name, *nn, dur);
                    Some(needs)
                }
            }
        }
    })
    .spawn(1)
}

async fn async_sleep() -> Duration {
    let ms = rand::thread_rng().gen_range(0..1000);
    let dur = Duration::from_millis(ms);
    tokio::time::sleep(dur).await;
    dur
}

fn rand_u32(s: u32, e: u32) -> u32 {
    rand::thread_rng().gen_range(s..e)
}
