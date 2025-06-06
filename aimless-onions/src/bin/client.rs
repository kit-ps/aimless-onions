#![feature(try_blocks)]
use std::{fs::OpenOptions, io::{self, BufRead}, path::PathBuf, sync::Arc, thread, time::Instant};

use aimless_onions::{
    apitypes::GetRelayAddress, format::{self, Identity, Onion}, shared::{self, Message, Epoch}
};
use clap::Parser;
use color_eyre::{eyre::{bail, eyre, Error}, Result};
use csv::Writer;
use hohibe::kem::PublicKey;
use rand::Rng;
use rand_distr::{Alphanumeric, Distribution, Exp};
use reqwest::{Certificate, Url};
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::{mpsc, RwLock}};
use tracing::{error, info};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Authority {
    address: String,
    cert: PathBuf,
}

impl Authority {
    fn get_relay_address_url(&self) -> Url {
        Url::parse(&self.address)
            .unwrap()
            .join("get-relay-address")
            .unwrap()
    }

    fn get_current_public_key_url(&self) -> Url {
        Url::parse(&self.address)
            .unwrap()
            .join("get-current-public-key")
            .unwrap()
    }

    fn get_next_public_key_url(&self) -> Url {
        Url::parse(&self.address)
            .unwrap()
            .join("get-next-public-key")
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
    authority: Vec<Authority>,
}

#[derive(Parser, Debug, Clone)]
struct Cli {
    #[arg(short, long, default_value = "client.toml")]
    config: PathBuf,
    #[arg(short, long, default_value_t = false)]
    interactive: bool,
    #[arg(short, long, default_value_t = 100)]
    send_interval: u16,
    #[arg(short, long, default_value_t = 10000)]
    message_count: usize,
}

async fn request_relay(authority: &Authority, onion: &Onion) -> Result<(String, u16), Error> {
    let resp = authority
        .client().await.expect("Could not construct client")
        .post(authority.get_relay_address_url())
        .json(&GetRelayAddress { identity: onion.identity, })
        .send().await;
    match resp {
        Ok(resp) => {
            let resp = resp.json().await;
            match resp {
                Ok::<(String, u16), _>((address, port)) => {
                    Ok((address, port))
                }
                Err(e) => {
                    Err(eyre!(e))
                }
            }
        }
        Err(e) => {
            Err(eyre!(e))
        }
    }
}

async fn send_onion(authorities: &[Authority], onion: &Onion) -> Result<()> {
    for authority in authorities {
        info!("Requesting relay address for {}", onion.identity);
        let relay = request_relay(&authority, &onion).await;
        info!("Address is {:?}", relay);
        if relay.is_err() {
            info!("Authority {} did not return a relay. Trying next authority. Error: {}", &authority.address, relay.err().unwrap());
            continue
        }
        let (address, port) = relay.unwrap();
        let resp = reqwest::Client::new()
            .post(Url::parse(format!("http://{}:{}", &address, port).as_str()).unwrap().join("relay").unwrap())
            .json(onion)
            .send().await;
        match resp {
            Ok(_) => return Ok(()),
            Err(e) => bail!("Failed to send onion to first relay: {}", e),
        }
    }
    Err(eyre!("No authority returned a valid relay address"))
}

type PublicKeys = Arc<RwLock<Vec<PublicKey>>>;

fn random_msg_iterator() -> impl Iterator<Item = String> {
    std::iter::repeat_with(|| {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(rand::thread_rng().gen_range(1..=20))
            .map(char::from)
            .collect::<String>()
    })
}

async fn build_and_send(config: &Config, public_keys: &[PublicKey], text: String) -> Result<()> {
    let mut rng = rand::thread_rng();
    let identities: &[Identity] = &[rng.gen(), rng.gen(), rng.gen()];

    let exp = Exp::new(1.0).unwrap();

    if public_keys.len() < 1 {
        bail!("Need at least 1 available authority to send an onion. Aborting.");
    }

    let message = Message {
        timestamp: shared::timestamp(),
        content: text,
    };
    let delays: Vec<u32> = exp.sample_iter(&mut rng)
        .take(identities.len())
        .map(|f_delay| {f_delay * 1000.0} as u32) //reinterpret f64 in s to u32 in ms
        .collect();
    let now = Instant::now();
    info!("Using {} identities, {} delays, {} authority public keys to send onion with message {}", identities.len(), delays.len(), public_keys.len(), message.content);
    let onion = format::wrap(&mut rng, identities, &delays, &public_keys, &bincode::serialize(&message).unwrap()[..])?;
    let elapsed = now.elapsed();
    send_onion(&config.authority, &onion).await?;
    println!("--> Onion sent, wrapping took {}ms!", elapsed.as_millis());

    // Save the wrapping time in a csv
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("wrapping_times.csv")?;
    let mut writer = Writer::from_writer(file);
    writer.serialize((&message.content, elapsed.as_millis()))?;
    writer.flush()?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CurrentOrNext {
    Current,
    Next,
}

async fn retrieve_keys(config: &Config, current_or_next: CurrentOrNext) -> Result<Vec<PublicKey>> {
    let mut public_keys = Vec::new();
    for authority in &config.authority {
        info!("Getting keys from {:?}", authority);
        let client = authority.client().await?;
        let url = match current_or_next {
            CurrentOrNext::Current => authority.get_current_public_key_url(),
            CurrentOrNext::Next => authority.get_next_public_key_url(),
        };
        let res = client.get(url).send().await?;
        if res.status() != 200 {
            bail!("We couldn't get the key of {:?}", authority);
        };
        public_keys.push(res.json().await?);
    }
    info!("Got {} keys.", public_keys.len());
    Ok(public_keys)
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().init();
    let cli = Cli::parse();
    let config: Config = toml::from_str(&fs::read_to_string(&cli.config).await.expect("client.toml not found."))?;

    let public_keys = PublicKeys::default();

    info!("Getting initial keys...");
    public_keys.write().await.extend(retrieve_keys(&config, CurrentOrNext::Current).await?);

    {
        let config = config.clone();
        let public_keys = public_keys.clone();
        let mut epoch = Epoch::next();

        tokio::spawn(async move {
            loop {
                let result: Result<()> = try {
                    info!("Waiting for next epoch allocations to finish.");
                    epoch.wait_for_allocation_finish().await;
                    info!("Getting public keys for next epoch.");
                    let next_public_keys = retrieve_keys(&config, CurrentOrNext::Next).await?;

                    info!("Waiting for epoch switch over.");
                    epoch.wait_for_switchover().await;
                    info!("Switching epoch.");
                    let mut public_keys = public_keys.write().await;
                    public_keys.clear();
                    public_keys.extend(next_public_keys);
                    epoch = Epoch::next();
                };
                if let Err(e) = result {
                    error!("Error: {}", e);
                }
            }
        });
    }

    if cli.interactive {
        let (sender, mut receiver) = mpsc::channel(10);

        thread::spawn(move || {
            println!("Enter messages and confirm with enter:");
            for input in io::stdin().lock().lines() {
                match input {
                    Ok(v) => {
                        sender.blocking_send(v).unwrap();
                    },
                    Err(_) => {
                        println!("Invalid input!");
                        continue
                    },
                };
            }
        });

        loop {
            let Some(line) = receiver.recv().await else {
                break
            };

            if let Err(e) = build_and_send(&config, &public_keys.read().await, line).await {
                error!("Error: {}", e);
            }
        }
    } else {
        info!("Sending random onions every {}ms", cli.send_interval);
        for (_, message) in random_msg_iterator().take(cli.message_count).enumerate() {
            if let Err(e) = build_and_send(&config, &public_keys.read().await, message).await {
                error!("Error: {}", e);
            }
            tokio::time::sleep(std::time::Duration::from_millis(cli.send_interval as u64)).await;
        }
    }

    Ok(())
}
