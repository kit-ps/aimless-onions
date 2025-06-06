use aimless_onions::{format, nodename::NodenameMapper};
use color_eyre::Result;
use hohibe::kem::HybridKem;
use rand::Rng;

const MAX_AUTHORITIES: usize = 9;
const MAX_PATH: usize = 5;

static PAYLOAD: &[u8] = include_bytes!("../../benches/payload.txt");

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut rng = rand::thread_rng();
    let kem = HybridKem::new_with_mapper(32, NodenameMapper);
    let authorities = (0..MAX_AUTHORITIES)
        .map(|_| kem.setup(&mut rng).unwrap())
        .collect::<Vec<_>>();
    let public_keys = authorities.iter().map(|a| a.0.clone()).collect::<Vec<_>>();

    let path = (0..MAX_PATH).map(|_| rng.gen::<format::Identity>()).collect::<Vec<_>>();
    let delays = (0..MAX_PATH).map(|_| rng.gen::<u32>()).collect::<Vec<_>>();

    println!("path_length,authorities,payload_size,onion_size");
    for path_length in 1..=MAX_PATH {
        for authority_count in 1..=MAX_AUTHORITIES {
            for payload_size in [1, 512, 1024, 2048, 4069] {
                let public_keys = &public_keys[..authority_count];
                let payload = &PAYLOAD[..payload_size];
                let path = &path[..path_length];
                let delays = &delays[..path_length];
                let onion = format::wrap(&mut rng, path, delays, public_keys, payload).unwrap();

                let size = bincode::serialize(&onion)?.len();
                println!("{path_length},{authority_count},{payload_size},{size}");
            }
        }
    }

    Ok(())
}
