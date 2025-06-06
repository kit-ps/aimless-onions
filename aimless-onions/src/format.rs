//! Prototype onion format.
//!
//! This basically just combines the layered encryption with IBE and secret sharing.
use std::{iter, mem};

use aes::cipher::{KeyIvInit, StreamCipher};
use color_eyre::{eyre::bail, Result};
use hmac::{Hmac, Mac};
use hohibe::kem::{HybridKem, PrivateKey, PublicKey};
use once_cell::sync::Lazy;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use sha3::Sha3_256;
use shamir_secret_sharing::{
    num_bigint::{BigInt, Sign},
    ShamirSecretSharing,
};

use crate::nodename::{NodeName, NodenameMapper, HIERARCHY_DEPTH};

// This must be a prime that is large enough to contain 128 bit numbers.
static PRIME_STR: &[u8] = b"927659228076472818176252176283652096798126523793";

pub type Identity = u64;
type ShaHmac = Hmac<Sha3_256>;
pub type Tag = [u8; 32];

fn tag_onion(key: &[u8], identity: Identity, shares: &[Share], header: &[u8], payload: &[u8]) -> Tag {
    let mut mac = ShaHmac::new_from_slice(key).unwrap();
    mac.update(&identity.to_le_bytes());
    mac.update(&shares.len().to_le_bytes());
    for share in shares {
        mac.update(&share.0);
    }
    // We assume that the MAC is at the right spot, so we ignore it when feeding data into the
    // calculation.
    mac.update(&header[..HOP_MACCED_PREFIX_LEN]);
    mac.update(&header[HOP_INFO_SIZE..]);
    mac.update(payload);
    mac.finalize().into_bytes().into()
}

fn access_tag(header: &mut [u8]) -> &mut [u8] {
    &mut header[HOP_MACCED_PREFIX_LEN..HOP_MACCED_PREFIX_LEN + mem::size_of::<Tag>()]
}

fn random_nonce<R: Rng + CryptoRng>(mut rng: R) -> [u8; 16] {
    rng.gen()
}

// The prime has 160 bits
const INT_SIZE: usize = 160 / 8;
// G1Affine + G2Affine + Vec length
const HIBE_OVERHEAD: usize = 48 + 96 + 8;
const SHARE_SIZE: usize = INT_SIZE + HIBE_OVERHEAD;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Share(#[serde(with = "serde_arrays")] pub [u8; SHARE_SIZE]);

impl Share {
    pub fn empty() -> Share {
        Share([0; SHARE_SIZE])
    }

    pub fn is_empty(&self) -> bool {
        self == &Share::empty()
    }

    pub fn wrap<R: Rng + CryptoRng>(
        rng: R,
        identity: Identity,
        public_key: &PublicKey,
        share: &BigInt,
    ) -> Share {
        let mut share_bytes = [0u8; INT_SIZE];
        // We use little endian because that allows us to easily "pad" the number with zeroes
        // (simply by copying the slice from the left).
        let encoded_int = share.to_bytes_le().1;

        assert!(encoded_int.len() <= share_bytes.len());
        share_bytes[..encoded_int.len()].copy_from_slice(&encoded_int);

        let encrypted = BBG
            .encrypt(rng, public_key, NodeName::number(identity.into()), &share_bytes)
            .unwrap();
        Share(encrypted.try_into().unwrap())
    }

    pub fn unwrap(&self, public_key: &PublicKey, private_key: &PrivateKey) -> Result<BigInt> {
        let decrypted = BBG.decrypt(public_key, private_key, &self.0)?;
        Ok(BigInt::from_bytes_le(Sign::Plus, &decrypted))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Onion {
    pub identity: Identity,
    pub shares: Vec<Share>,
    pub header: Vec<u8>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[repr(C)]
pub struct HopInfo {
    pub delay: u32,
    pub tag: Tag,
}

const HOP_MACCED_PREFIX_LEN: usize = mem::offset_of!(HopInfo, tag);
const HOP_INFO_SIZE: usize = mem::size_of::<HopInfo>();

const fn per_hop_size(num_authorities: usize) -> usize {
    HOP_INFO_SIZE + /* identity */ std::mem::size_of::<Identity>() + /* vec length in shares */ 8 + num_authorities * SHARE_SIZE
}

type AesCtr = ctr::Ctr64LE<aes::Aes128>;
static IV: [u8; 16] = [0; 16];

static BBG: Lazy<HybridKem<NodenameMapper>> =
    Lazy::new(|| HybridKem::new_with_mapper(HIERARCHY_DEPTH.into(), NodenameMapper));

impl Onion {
    pub fn fresh(num_authorities: usize, path_length: usize, data: Vec<u8>) -> Onion {
        Onion {
            identity: 0,
            shares: (0..num_authorities).map(|_| Share::empty()).collect(),
            header: vec![0; path_length * per_hop_size(num_authorities)],
            payload: data,
        }
    }

    pub fn compute_filler(&mut self, nonce: &[u8; 16]) {
        let mut cipher = AesCtr::new(nonce[..16].try_into().unwrap(), &IV.into());

        let phs = per_hop_size(self.shares.len());

        self.header.extend(iter::repeat(0).take(phs));

        cipher.apply_keystream(&mut self.header);

        self.header.drain(0..phs);
    }

    pub fn wrap<R: Rng + CryptoRng>(
        &self,
        mut rng: R,
        identity: Identity,
        public_keys: &[PublicKey],
        delay: u32,
    ) -> Onion {
        let nonce = random_nonce(&mut rng);
        self.wrap_with_nonce(rng, &nonce, identity, public_keys, delay)
    }

    pub fn wrap_with_nonce<R: Rng + CryptoRng>(
        &self,
        mut rng: R,
        nonce: &[u8; 16],
        identity: Identity,
        public_keys: &[PublicKey],
        delay: u32,
    ) -> Onion {
        let sss = ShamirSecretSharing {
            threshold: public_keys.len(),
            share_amount: public_keys.len() + 1,
            prime: BigInt::parse_bytes(PRIME_STR, 10).unwrap(),
        };
        let shares = sss.split(BigInt::from_bytes_le(Sign::Plus, nonce));

        let shares = shares
            .iter()
            .zip(public_keys.iter())
            .map(|((_idx, share), public_key)| Share::wrap(&mut rng, identity, public_key, share))
            .collect::<Vec<_>>();

        let hop = HopInfo {
            delay,
            tag: Default::default(),
        };

        let mut header = bincode::serialize(&(&hop, self.identity, &self.shares)).unwrap();

        assert_eq!(header.len(), per_hop_size(public_keys.len()));
        header.extend(&self.header);

        let mut cipher = AesCtr::new(nonce[..16].try_into().unwrap(), &IV.into());
        cipher.apply_keystream(&mut header);
        header.drain(self.header.len()..);

        assert_eq!(header.len(), self.header.len());

        let mut payload = self.payload.clone();
        cipher.apply_keystream(&mut payload);

        let tag = tag_onion(nonce, identity, &shares, &header, &payload);
        access_tag(&mut header).copy_from_slice(&tag);

        Onion {
            identity,
            shares,
            header,
            payload,
        }
    }

    pub fn unwrap(
        mut self,
        public_keys: &[PublicKey],
        private_keys: &[PrivateKey],
    ) -> Result<(HopInfo, Onion)> {
        let sss = ShamirSecretSharing {
            threshold: self.shares.len(),
            share_amount: self.shares.len() + 1,
            prime: BigInt::parse_bytes(PRIME_STR, 10).unwrap(),
        };

        let shares = self
            .shares
            .iter()
            .zip(public_keys.iter())
            .zip(private_keys.iter())
            .map(|((share, public_key), private_key)| share.unwrap(public_key, private_key))
            .collect::<Result<Vec<_>, _>>()?;

        let shares = shares
            .into_iter()
            .enumerate()
            .map(|(idx, share)| (idx + 1, share))
            .collect::<Vec<_>>();

        let secret = sss.recover(&shares);
        let mut nonce = secret.to_bytes_le().1;

        // If our sampled secret doesn't use all bytes (the first digits are zero), we need to fill
        // the lower-endian bytes with 0.
        while nonce.len() < 16 {
            nonce.push(0);
        }

        let tag = tag_onion(
            &nonce,
            self.identity,
            &self.shares,
            &self.header,
            &self.payload,
        );
        if tag != access_tag(&mut self.header) {
            bail!("MAC mismatch");
        }

        let mut cipher = AesCtr::new(nonce[..16].try_into().unwrap(), &IV.into());

        let phs = per_hop_size(public_keys.len());

        self.header.extend(iter::repeat(0).take(phs));

        cipher.apply_keystream(&mut self.header);
        cipher.apply_keystream(&mut self.payload);

        let this_header = self.header.drain(0..phs).collect::<Vec<_>>();
        let (hop, identity, shares): (HopInfo, Identity, Vec<Share>) =
            bincode::deserialize(&this_header)?;

        self.identity = identity;
        self.shares = shares;

        Ok((hop, self))
    }

    pub fn is_final_destination(&self) -> bool {
        self.identity == 0 && self.shares.iter().all(|share| share.is_empty())
    }
}

/// Wrap an onion in multiple layers, for all given identities.
///
/// The public keys are the keys of the authorities.
///
/// Returns the final onion.
pub fn wrap<R: Rng + CryptoRng>(
    mut rng: R,
    identities: &[Identity],
    delays: &[u32],
    public_keys: &[PublicKey],
    data: &[u8],
) -> Result<Onion> {
    if identities.len() != delays.len() {
        bail!("Mismatching number of delays and identities");
    }

    let nonces = identities
        .iter()
        .map(|_| random_nonce(&mut rng))
        .collect::<Vec<_>>();

    let mut onion = Onion::fresh(public_keys.len(), identities.len(), Vec::from(data));
    for nonce in &nonces {
        onion.compute_filler(nonce);
    }
    for ((identity, delay), nonce) in identities
        .iter()
        .rev()
        .zip(delays.iter().rev())
        .zip(nonces.iter().rev())
    {
        onion = onion.wrap_with_nonce(&mut rng, nonce, *identity, public_keys, *delay);
    }
    Ok(onion)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn wrap_unwrap() {
        let mut rng = rand::thread_rng();
        let identity: Identity = 0xDEADCAFE;
        let authorities = [BBG.setup(&mut rng).unwrap(), BBG.setup(&mut rng).unwrap()];
        let data: &[u8] = b"The quick brown fox jumps over the lazy dog";

        let public_keys = authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
        let mut onion = Onion::fresh(2, 1, Vec::from(data));
        let original_size = bincode::serialized_size(&onion).unwrap();
        onion = onion.wrap(&mut rng, identity, &public_keys, 1337);

        assert_eq!(bincode::serialized_size(&onion).unwrap(), original_size);

        let private_keys = authorities
            .iter()
            .map(|a| {
                BBG.generate_key(&mut rng, &a.0, &a.1, NodeName::number(identity.into()))
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let (unwrapped_hop, unwrapped_onion) = onion.unwrap(&public_keys, &private_keys).unwrap();

        assert_eq!(
            bincode::serialized_size(&unwrapped_onion).unwrap(),
            original_size
        );
        assert_eq!(unwrapped_hop.delay, 1337);
        assert_eq!(unwrapped_onion.payload, data);
    }

    #[test]
    fn wrap_unwrap_tagged() {
        let mut rng = rand::thread_rng();
        let identity: Identity = 0xDEADCAFE;
        let authorities = [BBG.setup(&mut rng).unwrap(), BBG.setup(&mut rng).unwrap()];
        let data: &[u8] = b"The quick brown fox jumps over the lazy dog";

        let public_keys = authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
        let mut onion = Onion::fresh(2, 1, Vec::from(data));
        onion = onion.wrap(&mut rng, identity, &public_keys, 1337);

        onion.payload[0] ^= 0x01;

        let private_keys = authorities
            .iter()
            .map(|a| {
                BBG.generate_key(&mut rng, &a.0, &a.1, NodeName::number(identity.into()))
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let unwrapped = onion.unwrap(&public_keys, &private_keys);
        assert!(unwrapped.is_err());
    }

    #[test]
    fn wrap_unwrap_multiple() {
        let mut rng = rand::thread_rng();
        let identities: &[Identity] = &[0xCAFEBABE, 0xDEADBEEF, 0xC001C0DE];
        let delays = &[0x4D454F57u32, 0x4D415242, 0x63825363];
        let authorities = [BBG.setup(&mut rng).unwrap(), BBG.setup(&mut rng).unwrap()];
        let data: &[u8] = b"The quick brown fox jumps over the lazy dog";

        let public_keys = authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
        let mut onion = wrap(&mut rng, identities, delays, &public_keys, data).unwrap();
        let onion_size = bincode::serialized_size(&onion).unwrap();

        for (identity, delay) in identities.iter().zip(delays.iter()) {
            let private_keys = authorities
                .iter()
                .map(|a| {
                    BBG.generate_key(&mut rng, &a.0, &a.1, NodeName::number((*identity).into()))
                        .unwrap()
                })
                .collect::<Vec<_>>();

            let hop_info;
            (hop_info, onion) = onion.unwrap(&public_keys, &private_keys).unwrap();
            assert_eq!(hop_info.delay, *delay);
            assert_eq!(bincode::serialized_size(&onion).unwrap(), onion_size);
        }

        assert!(onion.is_final_destination());
        assert_eq!(onion.payload, data);
    }
}
