#![feature(try_blocks)]
use std::{collections::HashMap, convert::Infallible, path::PathBuf, sync::Arc, thread, time::Duration};

use aimless_onions::{
    apitypes::{GetHibeKeys, GetRelayAddress, KeyPair, RegisterRelay}, format::{Identity, Onion}, hibe::CachedBbgKeygen, nodename::{NodeName, NodenameMapper}, shared::{self, Epoch, Message}
};
use clap::Parser;
use color_eyre::{eyre::bail, Result};
use hohibe::{kem::{PrivateKey, PublicKey}, Mapper};
use rand::Rng;
use reqwest::{Certificate, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{fs, sync::RwLock, task, runtime::Handle};
use tracing::{error, info};
use warp::Filter;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Authority {
    address: String,
    cert: PathBuf,
}

impl Authority {
    fn register_url(&self) -> Url {
        Url::parse(&self.address).unwrap().join("register").unwrap()
    }

    fn get_relay_address_url(&self) -> Url {
        Url::parse(&self.address)
            .unwrap()
            .join("get-relay-address")
            .unwrap()
    }

    fn get_next_public_key_url(&self) -> Url {
        Url::parse(&self.address)
            .unwrap()
            .join("get-next-public-key")
            .unwrap()
    }

    fn get_keys_url(&self) -> Url {
        Url::parse(&self.address)
            .unwrap()
            .join("get-hibe-keys")
            .unwrap()
    }

    async fn read_certificate(&self) -> Result<Certificate> {
        let bytes = fs::read(&self.cert).await?;
        Ok(Certificate::from_pem(&bytes)?)
    }

    async fn client(&self) -> Result<reqwest::Client> {
        Ok(reqwest::ClientBuilder::new()
            .danger_accept_invalid_hostnames(true)
            .add_root_certificate(self.read_certificate().await?)
            .build()?)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    weight: u32,
    public_address: String,
    port: u16,
    board_address: String,
    board_port: u16,
    authority: Vec<Authority>,
}

#[derive(Parser, Debug, Clone)]
struct Cli {
    #[arg(short, long, default_value = "relay.toml")]
    config: PathBuf,
}

#[derive(Debug, Clone)]
struct LiveAuthority {
    config: Authority,
    used_key: [u8; 16],
    public_key: PublicKey,
    received_keys: HashMap<NodeName, (CachedBbgKeygen, PrivateKey)>,
}

type LiveAuthorities = Arc<RwLock<Vec<LiveAuthority>>>;

impl LiveAuthority {
    fn get_private_key(&self, identity: Identity) -> Option<PrivateKey> {
        let node = NodeName::number(identity.into());
        for (name, (cache, key)) in self.received_keys.iter() {
            if name.contains(node) {
                let mut key = key.clone();
                for subnode in node.walk() {
                    if subnode.len() <= name.len() {
                        continue;
                    }
                    let identity = NodenameMapper.map_identity(subnode).unwrap();
                    let (child, identity) = identity.split_last().unwrap();
                    key = cache
                        .derive_key(&mut rand::thread_rng(), key.into(), &identity, &child)
                        .unwrap()
                        .into();
                }
                return Some(key);
            }
        }
        None
    }
}

async fn send_onion(live_authorities: LiveAuthorities, onion: Onion) -> Result<()> {
    let live_authorities = live_authorities.read().await;
    let authority = live_authorities.first().unwrap();
    let (address, port): (String, u16) = authority
        .config
        .client()
        .await?
        .post(authority.config.get_relay_address_url())
        .json(&GetRelayAddress {
            identity: onion.identity,
        })
        .send()
        .await?
        .json()
        .await?;

    reqwest::Client::new()
        .post(Url::parse(format!("http://{}:{}", &address, port).as_str()).unwrap().join("relay").unwrap())
        .json(&onion)
        .send()
        .await?;

    Ok(())
}

async fn relay_onion(
    board_url: Url,
    live_authorities: LiveAuthorities,
    onion: Onion,
) -> Result<impl warp::Reply, Infallible> {
    info!("Received onion. Relaying.");
    let rt = Handle::current();

    // Do everything in a separate pool so we can signal the client quickly.
    task::spawn_blocking(move || {
        let result: Result<()> = try {
            info!("Collecting authority public keys.");
            let public_keys = rt.block_on(live_authorities.read())
                .iter()
                .map(|live_authority| live_authority.public_key.clone())
                .collect::<Vec<_>>();
            info!("Requesting private keys for onion {}", onion.identity);
            let private_keys = rt.block_on(live_authorities.read())
                .iter()
                .filter_map(|live_authority| {
                    let sk = live_authority.get_private_key(onion.identity);
                    if sk.is_none() {
                        info!("sk from authority {} for onion {} is None.", live_authority.config.address, onion.identity);
                    }
                    sk
                })
                .collect::<Vec<_>>();
            info!("Using {} pks, {} sks", public_keys.len(), private_keys.len());
            let (hop_info, onion) = onion.unwrap(&public_keys, &private_keys)?;
            info!("Unwrapped onion successfully.");

            if onion.is_final_destination() {
                let message: Message = bincode::deserialize(&onion.payload[..]).unwrap();
                rt.block_on(reqwest::Client::new()
                    .post(board_url)
                    .json(&message)
                    .send())?;
                info!("Relayed message to board");
            } else {
                info!("Delaying onion to {} for {}ms", onion.identity, hop_info.delay);
                thread::sleep(Duration::from_millis(hop_info.delay as u64));
                info!("Finished waiting for onion to {}, sending.", onion.identity);
                rt.block_on(send_onion(live_authorities, onion))?;
            }
        };

        if let Err(e) = result {
            error!("{}", e);
        }
    });

    Ok(warp::reply::json(&json!({"status": "ok"})))
}

async fn register(config: &Config, used_keys: &mut Vec<[u8; 16]>) -> Result<()> {
    let used_key = rand::thread_rng().gen();
    for authority in &config.authority {
        let client = authority.client().await?;
        let registration_message = RegisterRelay {
            key: used_key,
            address: config.public_address.clone(),
            port: config.port,
            weight: config.weight,
        };
        used_keys.push(registration_message.key);
        let res = client
            .post(authority.register_url())
            .json(&registration_message)
            .send()
            .await;
        if res.is_err() || res.unwrap().status() != 200 {
            error!("We couldn't register with {:?}", authority);
        } else {
            info!("Registered with {}", authority.address);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().init();
    let cli = Cli::parse();
    let config: Config = toml::from_str(&fs::read_to_string(&cli.config).await?)?;


    let live_authorities = LiveAuthorities::default();

    {
        let config = config.clone();
        let used_keys = Vec::new();
        let live_authorities = live_authorities.clone();
        tokio::spawn(async move {
            let mut epoch = Epoch::next();
            loop {
                info!("Epoch: {:?}", epoch);
                info!("Waiting for next registration window");
                epoch.wait_for_registration().await;
                info!("Registering with authorities");
                let mut used_keys = used_keys.clone();
                register(&config, &mut used_keys).await?;
                info!("Registration with all authorities finished");

                info!("Waiting for allocations to finish.");
                epoch.wait_for_allocation_finish().await;
                info!("Getting new allocations.");

                let mut next_authorities: Vec<LiveAuthority> = Vec::new();
            
                for (authority, used_key) in config.authority.iter().zip(used_keys.into_iter()) {
                    let client = authority.client().await?;
                    let res = client.get(authority.get_next_public_key_url()).send().await?;
                    if res.status() != 200 {
                        bail!("We couldn't get the key of {:?}", authority);
                    };
                    let public_key: PublicKey = res.json().await?;
                    let res = client
                        .post(authority.get_keys_url())
                        .json(&GetHibeKeys {
                            key: used_key.clone(),
                        })
                        .send()
                        .await?
                        .json::<Vec<KeyPair>>()
                        .await?;
                    let received_keys = res
                        .into_iter()
                        .map(|pair| {
                            let idmatrix = NodenameMapper::identity_matrix();
                            let idmatrix = idmatrix.iter().map(|x| x.as_slice()).collect::<Vec<_>>();
                            let cached = CachedBbgKeygen::generate(32, &public_key.clone().into(), &idmatrix).unwrap();
                            (pair.node, (cached, PrivateKey::from(pair.key)))
                        })
                        .collect::<HashMap<_, _>>();

                    next_authorities.push(LiveAuthority {
                        config: authority.clone(),
                        public_key,
                        used_key,
                        received_keys,
                    });
                }
                    
                info!("Waiting for next epoch");
                epoch.wait_for_switchover().await;
                info!("Reached next epoch, switching keys");
                epoch = Epoch::next();
        
                let mut live_authorities = live_authorities.write().await;
                live_authorities.clear();
                live_authorities.append(&mut next_authorities);
                info!("Updated {} live authorities.", live_authorities.len());
            }
            #[allow(unreachable_code)]
            Ok(())
        });
    }

    info!("Starting relay work");
    let relay = warp::path!("relay")
        .and(warp::any().map(move || Url::parse(format!("http://{}:{}", &config.board_address, config.board_port).as_str()).unwrap().join("relay").unwrap()))
        .and(warp::any().map(move || live_authorities.clone()))
        .and(warp::body::json())
        .then(relay_onion);

    warp::serve(relay).run(shared::parse_socket_addr("0.0.0.0", config.port).expect("Invalid socket address given")).await;
    Ok(())
}
