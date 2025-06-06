use aimless_onions::{
    allocation::{allocate, AllocationRequest},
    consensus,
};

use color_eyre::Result;

fn main() -> Result<()> {
    let relays = consensus::read("tor-consensus")?
        .into_iter()
        .enumerate()
        .map(|(i, r)| AllocationRequest {
            id: i.try_into().unwrap(),
            key: Default::default(),
            weight: r.weight.into(),
        })
        .collect::<Vec<_>>();

    let allocation = allocate(&relays);

    let keycount = allocation.iter().map(|a| a.nodes.len()).collect::<Vec<_>>();

    println!("Max: {}", keycount.iter().max().unwrap());
    println!(
        "Min: {}",
        keycount.iter().filter(|c| **c != 0).min().unwrap()
    );
    println!(
        "Avg: {}",
        keycount.iter().sum::<usize>() as f32 / keycount.len() as f32
    );
    println!("#:   {}", keycount.len());

    Ok(())
}
