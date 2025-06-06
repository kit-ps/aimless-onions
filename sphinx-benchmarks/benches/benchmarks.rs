// Copyright 2020 Nym Technologies SA
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

extern crate sphinx_packet;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sphinx_packet::constants::{
    DESTINATION_ADDRESS_LENGTH, IDENTIFIER_LENGTH, NODE_ADDRESS_LENGTH, MAX_PATH_LENGTH,
};
use sphinx_packet::crypto::keygen;
use sphinx_packet::header::delays;
use sphinx_packet::route::{Destination, DestinationAddressBytes, Node, NodeAddressBytes};
use sphinx_packet::SphinxPacket;
use std::convert::TryInto;
use std::time::Duration;

fn make_packet_copy(packet: &SphinxPacket) -> SphinxPacket {
    SphinxPacket::from_bytes(&packet.to_bytes()).unwrap()
}

// two of those can be run concurrently to perform credential verification
fn bench_new_no_surb(c: &mut Criterion) {
    let nodes = (0..MAX_PATH_LENGTH)
        .map(|i| {
            let (_, pk) = keygen();
            Node::new(
                NodeAddressBytes::from_bytes([i.try_into().unwrap(); NODE_ADDRESS_LENGTH]),
                pk,
            )
        })
        .collect::<Vec<_>>();

    let delays = delays::generate_from_average_duration(nodes.len(), Duration::from_millis(10));
    let destination = Destination::new(
        DestinationAddressBytes::from_bytes([3u8; DESTINATION_ADDRESS_LENGTH]),
        [4u8; IDENTIFIER_LENGTH],
    );

    let message = vec![13u8, 16];

    c.bench_function("sphinx creation", |b| {
        b.iter(|| {
            SphinxPacket::new(
                black_box(message.clone()),
                black_box(&nodes),
                black_box(&destination),
                black_box(&delays),
            )
            .unwrap()
        })
    });
}

fn bench_unwrap(c: &mut Criterion) {
    let nodes = (0..MAX_PATH_LENGTH)
        .map(|i| {
            let (sk, pk) = keygen();
            (sk, Node::new(
                NodeAddressBytes::from_bytes([i.try_into().unwrap(); NODE_ADDRESS_LENGTH]),
                pk,
            ))
        })
        .collect::<Vec<_>>();

    let route = nodes.iter().map(|x| x.1.clone()).collect::<Vec<_>>();
    let delays = delays::generate_from_average_duration(route.len(), Duration::from_millis(10));
    let destination = Destination::new(
        DestinationAddressBytes::from_bytes([3u8; DESTINATION_ADDRESS_LENGTH]),
        [4u8; IDENTIFIER_LENGTH],
    );

    let message = vec![13u8, 16];
    let packet = SphinxPacket::new(message, &route, &destination, &delays).unwrap();

    let node1_sk = &nodes[0].0;

    // technically it's not benching only unwrapping, but also "make_packet_copy"
    // but it's relatively small
    c.bench_function("sphinx unwrap", |b| {
        b.iter(|| {
            make_packet_copy(&packet)
                .process(black_box(node1_sk))
                .unwrap()
        })
    });
}

criterion_group!(sphinx, bench_new_no_surb, bench_unwrap);

criterion_main!(sphinx);
