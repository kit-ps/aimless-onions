use std::iter;

use bls12_381_plus::{elliptic_curve::Field, G2Projective, Scalar};
use color_eyre::{eyre::bail, Result};
use fnv::FnvHashMap;
use hohibe::hibe::{BonehBoyenGoh, Hibe};
use rand::Rng;

type RawScalar = [u64; 4];

#[derive(Debug, Clone)]
pub struct CachedBbgKeygen {
    max_depth: usize,
    products: FnvHashMap<(usize, RawScalar), G2Projective>,
    public_key: <BonehBoyenGoh as Hibe>::PublicKey,
}

impl CachedBbgKeygen {
    pub fn generate(
        max_depth: usize,
        public_key: &<BonehBoyenGoh as Hibe>::PublicKey,
        identities: &[&[<BonehBoyenGoh as Hibe>::Identity]],
    ) -> Result<Self> {
        if identities.len() > max_depth {
            bail!("Identity too long");
        }
        let mut products = FnvHashMap::default();
        for (idx, (public_element, identity_elements)) in
            public_key.4.iter().zip(identities.iter()).enumerate()
        {
            for identity in *identity_elements {
                let product = public_element * identity;
                products.insert((idx, identity.to_raw()), product);
            }
        }
        Ok(CachedBbgKeygen {
            max_depth,
            products,
            public_key: public_key.clone(),
        })
    }

    #[inline]
    fn product(&self, index: usize, identity: &<BonehBoyenGoh as Hibe>::Identity) -> G2Projective {
        self.products
            .get(&(index, identity.to_raw()))
            .cloned()
            .unwrap_or_else(|| self.public_key.4[index] * identity)
    }

    pub fn generate_key<R: Rng>(
        &self,
        rng: R,
        master_key: &<BonehBoyenGoh as Hibe>::MasterKey,
        identity: &[<BonehBoyenGoh as Hibe>::Identity],
    ) -> Result<<BonehBoyenGoh as Hibe>::PrivateKey> {
        if identity.len() > self.max_depth {
            bail!("Identity too long");
        }

        let r = Scalar::random(rng);
        Ok((
            (master_key
                + (identity
                    .iter()
                    .enumerate()
                    .map(|(i, elem)| self.product(i, elem))
                    .sum::<G2Projective>()
                    + self.public_key.3)
                    * r)
                .into(),
            (self.public_key.0 * r).into(),
            self.public_key.4[identity.len()..]
                .iter()
                .map(|h| (h * r).into())
                .collect(),
        ))
    }

    pub fn derive_key<R: Rng>(
        &self,
        rng: R,
        mut parent_key: <BonehBoyenGoh as Hibe>::PrivateKey,
        parent_name: &[<BonehBoyenGoh as Hibe>::Identity],
        child: &<BonehBoyenGoh as Hibe>::Identity,
    ) -> Result<<BonehBoyenGoh as Hibe>::PrivateKey> {
        if parent_name.len() > self.max_depth - 1 {
            bail!("Identity too long");
        }

        let first = parent_key.2.remove(0);

        let t = Scalar::random(rng);

        for (b, h) in parent_key.2.iter_mut().zip(self.public_key.4[parent_name.len() + 1..].iter()) {
            *b = (*b + h * t).into();
        }

        Ok((
            (parent_key.0
                + first * child
                + (parent_name.iter().chain(iter::once(child))
                    .enumerate()
                    .map(|(i, elem)| self.product(i, elem))
                    .sum::<G2Projective>()
                    + self.public_key.3)
                    * t)
                .into(),
            (parent_key.1 + self.public_key.0 * t).into(),
            /*parent_key.2[1..]
                .iter()
                .zip(self.public_key.4[parent_name.len() + 1..].iter())
                .map(|(b, h)| b + h * t)
                .map(Into::into)
                .collect(),*/
            parent_key.2,
        ))
    }
}

#[cfg(test)]
mod test {
    use hohibe::hibe::HibeKem;

    use super::*;

    #[test]
    fn encapsulate_decapsulate_cached_keygen() {
        let mut rng = rand::thread_rng();
        let bbg = BonehBoyenGoh::new(5);
        let (public_key, master_key) = bbg.setup(&mut rng).unwrap();
        let cached_generator = CachedBbgKeygen::generate(
            5,
            &public_key,
            &[&[Scalar::from(1u32)], &[Scalar::from(2u32)], &[]],
        )
        .unwrap();
        let identity = &[Scalar::from(1u32), Scalar::from(2u32), Scalar::from(3u32)];
        let secret_key = cached_generator
            .generate_key(&mut rng, &master_key, identity.as_slice())
            .unwrap();
        let (generated_key, encapsulated_key) = bbg
            .encapsulate(&mut rng, &public_key, identity.as_slice())
            .unwrap();
        let decapsulated_key = bbg
            .decapsulate(&public_key, &secret_key, &encapsulated_key)
            .unwrap();
        assert_eq!(generated_key, decapsulated_key);
    }

    #[test]
    fn encapsulate_decapsulate_cached_derived() {
        let mut rng = rand::thread_rng();
        let bbg = BonehBoyenGoh::new(5);
        let (public_key, master_key) = bbg.setup(&mut rng).unwrap();
        let cached_generator = CachedBbgKeygen::generate(
            5,
            &public_key,
            &[&[Scalar::from(1u32)], &[Scalar::from(2u32)], &[]],
        )
        .unwrap();
        let identity = &[Scalar::from(1u32), Scalar::from(2u32), Scalar::from(3u32)];
        let parent_identity = &identity[..2];
        let parent_key = cached_generator
            .generate_key(&mut rng, &master_key, parent_identity)
            .unwrap();
        let secret_key = cached_generator
            .derive_key(&mut rng, parent_key, parent_identity, &Scalar::from(3u32))
            .unwrap();
        let (generated_key, encapsulated_key) = bbg
            .encapsulate(&mut rng, &public_key, identity.as_slice())
            .unwrap();
        let decapsulated_key = bbg
            .decapsulate(&public_key, &secret_key, &encapsulated_key)
            .unwrap();
        assert_eq!(generated_key, decapsulated_key);
    }
}
