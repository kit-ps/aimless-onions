use std::{
    net::{SocketAddr, ToSocketAddrs},
    time::SystemTime,
};

use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};

pub fn parse_socket_addr(address: &str, port: u16) -> Result<SocketAddr, std::io::Error> {
    let addr = format!("{}:{}", address, port);

    // Parse the address string into a SocketAddr
    let socket_addr = addr.to_socket_addrs()?.next();

    match socket_addr {
        Some(addr) => Ok(addr),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Failed to parse socket address",
        )),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub timestamp: u128,
    pub content: String,
}

pub fn timestamp() -> u128 {
    // Get the current time as a Duration since the Unix epoch
    let duration = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Timestamp failed, this should not happen");

    // Extract the number of seconds from the Duration
    duration.as_millis()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Epoch {
    registration_time: DateTime<Utc>,
    allocation_start_time: DateTime<Utc>,
    allocation_finish_time: DateTime<Utc>,
    switchover_time: DateTime<Utc>,
}

const EPOCH_DURATION: u32 = 60;

impl Epoch {
    pub fn from_registration(registration_time: DateTime<Utc>) -> Epoch {
        Epoch {
            registration_time,
            allocation_start_time: registration_time + Duration::minutes(10),
            allocation_finish_time: registration_time + Duration::minutes(15),
            switchover_time: registration_time + Duration::minutes(30),
        }
    }

    pub fn next() -> Epoch {
        let now = Utc::now();
        // Shift by 30 because we're looking at the registration time and not the "epoch start"
        let remaining_minutes =
            EPOCH_DURATION - ((now - Duration::minutes(30)).minute() % EPOCH_DURATION);
        let next_registration_time = (now + Duration::minutes(remaining_minutes.into()))
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();
        Epoch::from_registration(next_registration_time)
    }

    pub fn succeeding(&self) -> Epoch {
        Epoch::from_registration(self.registration_time + Duration::minutes(EPOCH_DURATION.into()))
    }

    pub async fn wait_for_registration(&self) {
        wait_until(self.registration_time).await;
    }

    pub async fn wait_for_allocation_start(&self) {
        wait_until(self.allocation_start_time).await;
    }

    pub async fn wait_for_allocation_finish(&self) {
        wait_until(self.allocation_finish_time).await;
    }

    pub async fn wait_for_switchover(&self) {
        wait_until(self.switchover_time).await;
    }
}

async fn wait_until(when: DateTime<Utc>) {
    let now = Utc::now();
    if when > now {
        tokio::time::sleep(
            when.signed_duration_since(now)
                .to_std()
                .expect("Duration may not be less than 0"),
        )
        .await;
    }
}
