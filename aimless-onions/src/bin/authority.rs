#![feature(async_closure, try_blocks)]

use std::{collections::HashMap, convert::Infallible, mem, path::PathBuf, sync::Arc};

use aimless_onions::{
    allocation::{self, Allocation, AllocationRequest},
    apitypes::{GetHibeKeys, GetRelayAddress, KeyPair, RegisterRelay, RelayKey},
    format::Identity,
    nodename::{NodeName, NodenameMapper},
    shared::{self, Epoch},
};
use clap::Parser;
use color_eyre::Result;
use hohibe::{
    hibe::{BonehBoyenGoh, Hibe},
    Mapper,
};
use once_cell::sync::Lazy;
use serde_json::json;
use tokio::sync::{RwLock, RwLockReadGuard};
use tracing::info;
use warp::Filter;

macro_rules! ise {
    ($($d:tt)*) => {
        {
            let result: Result<_> = try { $($d)* };
            match result {
                Ok(value) => Ok(Box::new(value) as Box<dyn warp::Reply>),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    Ok(Box::new(warp::http::StatusCode::INTERNAL_SERVER_ERROR))
                }
            }
        }
    }
}

static BBG: Lazy<BonehBoyenGoh> = Lazy::new(|| BonehBoyenGoh::new(32));

#[derive(Parser, Debug, Clone)]
struct Cli {
    address: String,
    key_file: PathBuf,
    cert_file: PathBuf,
    #[arg(long, default_value_t = 3030)]
    port: u16,
}

#[derive(Debug, Default)]
struct InnerRequests {
    requests: HashMap<RelayKey, AllocationRequest>,
    addresses: HashMap<RelayKey, (String, u16)>,
    counter: u32,
}

type Requests = Arc<RwLock<InnerRequests>>;
type RequestStore = (Requests, Requests);

#[derive(Debug, Default)]
struct InnerAllocations {
    allocations: HashMap<RelayKey, Allocation>,
    addresses: HashMap<RelayKey, (String, u16)>,
    public_params: <BonehBoyenGoh as Hibe>::PublicKey,
    master_key: <BonehBoyenGoh as Hibe>::MasterKey,
}

type Allocations = Arc<RwLock<InnerAllocations>>;
type AllocationStore = (Allocations, Allocations);

fn with<T: Clone + Send + Sync>(
    value: T,
) -> impl Filter<Extract = (T,), Error = Infallible> + Clone {
    warp::any().map(move || value.clone())
}

impl InnerRequests {
    async fn insert(&mut self, relay_key: RelayKey, address: String, port: u16, weight: u32) -> Result<()> {
        let next_id = self.counter;
        self.counter += 1;
        let request = AllocationRequest {
            id: next_id,
            key: relay_key,
            weight: weight.into(),
        };
        self.requests.insert(relay_key, request);
        self.addresses.insert(relay_key, (address, port));
        Ok(())
    }
    fn update_from(&mut self, requests: RwLockReadGuard<InnerRequests>) {
        self.addresses = requests.addresses.clone();
        self.requests = requests.requests.clone();
        self.counter = requests.counter;
    }
    fn clear(&mut self) {
        self.addresses = Default::default();
        self.requests = Default::default();
        self.counter = 0;
    }
}

impl InnerAllocations {
    async fn get_address(&self, identity: Identity) -> Option<(String, u16)> {
        let identity_node = NodeName::number(identity.into());
        for (key, allocation) in &self.allocations {
            if allocation
                .nodes
                .iter()
                .any(|node| node.contains(identity_node))
            {
                let (address, port) = self.addresses[key].clone();
                info!("Replying with address {}:{} for identity {}", address, port, identity);
                return Some((address, port));
            }
        }
        info!("Did not find an adress:port for identity {}", identity);
        None
    }
    fn update_from(&mut self, allocations: RwLockReadGuard<InnerAllocations>) {
        self.addresses = allocations.addresses.clone();
        self.allocations = allocations.allocations.clone();
        self.public_params = allocations.public_params.clone();
        self.master_key = allocations.master_key;
    }
    fn clear(&mut self) {
        self.addresses = Default::default();
        self.allocations = Default::default();
        self.public_params = Default::default();
        self.master_key = Default::default();
    }
}

async fn advance(reqs: RequestStore, allocs: AllocationStore) {
    let (current_requests, new_requests) = reqs;
    let (current_allocations, new_allocations) = allocs;
    current_requests.write().await.update_from(new_requests.read().await);
    current_allocations.write().await.update_from(new_allocations.read().await);
    new_requests.write().await.clear();
    new_allocations.write().await.clear();
}

async fn handle_request(
    requests: Requests,
    request: RegisterRelay,
) -> Result<impl warp::Reply, Infallible> {
    ise! {
        requests.write().await.insert(request.key, request.address, request.port, request.weight).await?;
        warp::reply::json(&json!({"status": "ok"}))
    }
}

async fn allocate(requests: Requests, allocations: Allocations) {
    let mut requests = requests.write().await;
    let mut allocations = allocations.write().await;
    allocations.allocations.clear();
    allocations.addresses = mem::take(&mut requests.addresses);

    (allocations.public_params, allocations.master_key) = BBG
        .setup(rand::thread_rng())
        .expect("BBG setup never fails");

    let reverse_keys = requests
        .requests
        .iter()
        .map(|(key, req)| (req.id, *key))
        .collect::<HashMap<_, _>>();
    let relay_requests = mem::take(&mut *requests)
        .requests
        .into_values()
        .collect::<Vec<_>>();
    let allocs = allocation::allocate(&relay_requests);

    for allocation in allocs {
        let key = reverse_keys[&allocation.id];
        allocations.allocations.insert(key, allocation);
    }

    requests.counter = 0;
}

async fn handle_get_relay_address(
    allocations: Allocations,
    request: GetRelayAddress,
) -> Result<impl warp::Reply, Infallible> {
    ise! {
        let allocations = allocations.read().await;
        warp::reply::json(&allocations.get_address(request.identity).await)
    }
}

async fn handle_get_hibe_keys(
    allocations: Allocations,
    request: GetHibeKeys,
) -> Result<impl warp::Reply, Infallible> {
    ise! {
        let allocations = allocations.read().await;
        let allocation = allocations.allocations.get(&request.key);

        let keys = allocation.map(|allocation| {
            allocation.nodes.iter()
                .map(|node| {
                    let mapped = NodenameMapper.map_identity(*node).unwrap();
                    let key = BBG.generate_key(rand::thread_rng(), &allocations.public_params, &allocations.master_key, &mapped).expect("BBG key generation shouldn't fail");
                    KeyPair { node: *node, key }
                })
                .collect::<Vec<_>>()
        }).or_else(|| Some(vec![])).expect("This should return an empty vec and not throw");

        warp::reply::json(&keys)
    }
}

async fn handle_get_public_key(allocations: Allocations) -> Result<impl warp::Reply, Infallible> {
    ise! {
        let allocations = allocations.read().await;
        warp::reply::json(&allocations.public_params)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().init();
    let cli = Cli::parse();

    let request_store = RequestStore::default();
    let allocation_store = AllocationStore::default();

    {
        let request_store = request_store.clone();
        let allocation_store = allocation_store.clone();
        tokio::spawn(async move {
            let mut epoch = Epoch::next();
            loop {
                info!("Epoch: {:?}", epoch);
                info!("Waiting for next allocation time");
                epoch.wait_for_allocation_start().await;
                let num_relays = request_store.1.read().await.addresses.len();
                if num_relays < 2 {
                    info!("Got only {} relay(s), skipping this epoch.", num_relays);
                } else {
                    info!("Doing allocations for next epoch");
                    allocate(request_store.1.clone(), allocation_store.1.clone()).await;
                    info!("Finished allocations. Waiting for epoch switch.");
                    epoch.wait_for_switchover().await;
                    advance(request_store.clone(), allocation_store.clone()).await;
                    info!("Switched to new epoch");
                    epoch = Epoch::next();
                }
            }
        });
    }

    let register = warp::path!("register")
        .and(with(request_store.1.clone()))
        .and(warp::body::json())
        .then(handle_request);

    let get_relay_address = warp::path!("get-relay-address")
        .and(with(allocation_store.0.clone()))
        .and(warp::body::json())
        .then(handle_get_relay_address);

    let get_hibe_keys = warp::path!("get-hibe-keys")
        .and(with(allocation_store.1.clone()))
        .and(warp::body::json())
        .then(handle_get_hibe_keys);

    let get_current_public_key = warp::path!("get-current-public-key")
        .and(with(allocation_store.0.clone()))
        .then(handle_get_public_key);

    let get_next_public_key = warp::path!("get-next-public-key")
        .and(with(allocation_store.1.clone()))
        .then(handle_get_public_key);

    let routes = register
        .or(get_relay_address)
        .or(get_hibe_keys)
        .or(get_current_public_key)
        .or(get_next_public_key);

    warp::serve(routes)
        .tls()
        .cert_path(&cli.cert_file)
        .key_path(&cli.key_file)
        .run(shared::parse_socket_addr("0.0.0.0", cli.port).expect("Invalid socket address given"))
        .await;
    Ok(())
}
