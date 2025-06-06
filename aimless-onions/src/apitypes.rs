use hohibe::hibe::{BonehBoyenGoh, Hibe};
use serde::{Deserialize, Serialize};

use crate::{format::Identity, nodename::NodeName};

pub type RelayKey = [u8; 16];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterRelay {
    pub key: RelayKey,
    pub address: String,
    pub port: u16,
    pub weight: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetHibeKeys {
    pub key: RelayKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRelayAddress {
    pub identity: Identity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyPair {
    pub node: NodeName,
    pub key: <BonehBoyenGoh as Hibe>::PrivateKey,
}
