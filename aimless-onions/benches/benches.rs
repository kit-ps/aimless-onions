use criterion::{
    criterion_group, criterion_main, BatchSize, Criterion, Throughput,
};

use aimless_onions::{
    format::{self, Identity},
    nodename::{HIERARCHY_DEPTH, NodeName, NodenameMapper}, consensus, allocation::{AllocationRequest, allocate}, hibe::CachedBbgKeygen,
};
use hohibe::{hibe::{BonehBoyenGoh, Hibe}, kem::{HybridKem, PublicKey}, Mapper};
use rand::{Rng, seq::SliceRandom};

static PAYLOAD: &[u8] = include_bytes!("payload.txt");

const MAX_AUTHORITIES: usize = 9;

fn bench_client(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let kem = HybridKem::new_with_mapper(HIERARCHY_DEPTH.into(), NodenameMapper);
    let authorities = (0..MAX_AUTHORITIES)
        .map(|_| kem.setup(&mut rng).unwrap())
        .collect::<Vec<_>>();
    let public_keys = authorities.iter().map(|a| a.0.clone()).collect::<Vec<_>>();

    let mut group = c.benchmark_group("client");

    let path = (0..5).map(|_| rng.gen::<Identity>()).collect::<Vec<_>>();
    let delays = (0..5).map(|_| rng.gen::<u32>()).collect::<Vec<_>>();

    for path_length in 1..=5 {
        for authority_count in 1..=MAX_AUTHORITIES {
            for payload_size in [512, 1024, 2048, 4069] {
                let path = &path[..path_length];
                let delays = &delays[..path_length];
                let public_keys = &public_keys[..authority_count];
                let payload = &PAYLOAD[..payload_size];
                group.bench_function(
                    format!("wrap/{path_length}/{authority_count}/{payload_size}"),
                    |b| b.iter(|| format::wrap(&mut rng, path, delays, public_keys, payload).unwrap()),
                );
            }
        }
    }
}

fn bench_relay(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let kem = HybridKem::new_with_mapper(HIERARCHY_DEPTH.into(), NodenameMapper);
    let authorities = (0..MAX_AUTHORITIES)
        .map(|_| kem.setup(&mut rng).unwrap())
        .collect::<Vec<_>>();
    let public_keys = authorities.iter().map(|a| a.0.clone()).collect::<Vec<_>>();

    let mut group = c.benchmark_group("relay");

    let path = (0..5).map(|_| rng.gen::<Identity>()).collect::<Vec<_>>();
    let delays = (0..5).map(|_| rng.gen::<u32>()).collect::<Vec<_>>();
    let private_keys = authorities
        .iter()
        .map(|(pk, mk)| {
            kem.generate_key(&mut rng, pk, mk, NodeName::number(path[0]))
                .unwrap()
        })
        .collect::<Vec<_>>();

    for authority_count in 1..=MAX_AUTHORITIES {
        for payload_size in [512, 1024, 2048, 4069] {
            let public_keys = &public_keys[..authority_count];
            let private_keys = &private_keys[..authority_count];
            let payload = &PAYLOAD[..payload_size];
            let onion = format::wrap(&mut rng, &path, &delays, public_keys, payload).unwrap();

            group.bench_function(format!("unwrap/{authority_count}/{payload_size}"), |b| {
                b.iter_batched(
                    || onion.clone(),
                    |onion| onion.unwrap(public_keys, private_keys).unwrap(),
                    BatchSize::SmallInput,
                )
            });
        }
    }

    let mut parent = NodeName::number(path[0]);
    for _ in 0..(HIERARCHY_DEPTH / 2) {
        parent = parent.parent();
    }
    let parent_keys = authorities
        .iter()
        .map(|(pk, mk)| {
            kem.generate_key(&mut rng, pk, mk, parent)
                .unwrap()
        })
        .collect::<Vec<_>>();

    for authority_count in 1..=MAX_AUTHORITIES {
        group.bench_function(format!("derive/{authority_count}"), |b| {
            b.iter(|| {
                for ((pk, _), parent_key) in authorities[..authority_count].iter().zip(parent_keys.iter()) {
                    let node = NodeName::number(path[0]);
                    let mut key = parent_key.clone();
                    for subnode in node.walk() {
                        if subnode.len() <= parent.len() {
                            continue;
                        }
                        key = kem
                            .derive_key(&mut rand::thread_rng(), pk, &key, subnode)
                            .unwrap();
                    }
                };
            });
        });
    }

    let idmatrix = NodenameMapper::identity_matrix();
    let idmatrix = idmatrix.iter().map(|x| x.as_slice()).collect::<Vec<_>>();
    let cached = authorities
        .iter()
        .map(|(pk, _)| CachedBbgKeygen::generate(HIERARCHY_DEPTH.into(), &<<BonehBoyenGoh as Hibe>::PublicKey as From<PublicKey>>::from(pk.clone()), &idmatrix).unwrap())
        .collect::<Vec<_>>();

    for authority_count in 1..=MAX_AUTHORITIES {
        group.bench_function(format!("derive_cached/{authority_count}"), |b| {
            b.iter(|| {
                for (cached, parent_key) in cached[..authority_count].iter().zip(parent_keys.iter()) {
                    let node = NodeName::number(path[0]);
                    let mut key = parent_key.clone();
                    for subnode in node.walk() {
                        if subnode.len() <= parent.len() {
                            continue;
                        }
                        let identity = NodenameMapper.map_identity(subnode).unwrap();
                        key = cached
                            .derive_key(&mut rand::thread_rng(), key.into(), &identity[..identity.len() - 1], identity.last().unwrap())
                            .unwrap()
                            .into();
                    }
                };
            });
        });
    }
}

fn bench_authority(c: &mut Criterion) {
    let mut rng = rand::thread_rng();

    let (pk, mk) = BonehBoyenGoh::new(HIERARCHY_DEPTH.into()).setup(&mut rng).unwrap();
    let identities = NodenameMapper::identity_matrix();
    let identities = identities.iter().map(|x| x.as_slice()).collect::<Vec<_>>();
    let cached = CachedBbgKeygen::generate(HIERARCHY_DEPTH.into(), &pk, &identities).unwrap();

    let mut group = c.benchmark_group("authority");
    group.throughput(Throughput::Elements(1));

    let relays = consensus::read("tor-consensus")
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(i, r)| AllocationRequest {
            id: i.try_into().unwrap(),
            key: Default::default(),
            weight: r.weight.into(),
        })
        .collect::<Vec<_>>();

    let allocation = allocate(&relays);

    group.bench_function("keygen", |b| {
        b.iter(|| {
            let idx = allocation.choose(&mut rng).unwrap();
            for node in &idx.nodes {
                let identity = NodenameMapper.map_identity(*node).unwrap();
                cached.generate_key(&mut rng, &mk, &identity).unwrap();
            }
        });
    });
}

criterion_group!(benches, bench_client, bench_relay, bench_authority);
criterion_main!(benches);
