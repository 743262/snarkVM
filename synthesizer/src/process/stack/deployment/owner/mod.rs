// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

mod bytes;
mod serialize;
mod string;

use console::{
    account::{Address, PrivateKey, Signature},
    network::prelude::*,
};

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Owner<N: Network> {
    /// The address of the owner.
    address: Address<N>,
    /// The signature of the owner, over the deployment transaction ID.
    signature: Signature<N>,
}

impl<N: Network> Owner<N> {
    /// Initializes a new owner.
    pub fn new<R: Rng + CryptoRng>(
        private_key: &PrivateKey<N>,
        transaction_id: N::TransactionID,
        rng: &mut R,
    ) -> Result<Self> {
        // Derive the address.
        let address = Address::try_from(private_key)?;
        // Sign the transaction ID.
        let signature = private_key.sign(&[*transaction_id], rng)?;
        // Return the owner.
        Ok(Self { signature, address })
    }

    /// Initializes a new owner from an address and signature.
    pub fn from(address: Address<N>, signature: Signature<N>) -> Self {
        Self { address, signature }
    }

    /// Returns the address of the owner.
    pub const fn address(&self) -> Address<N> {
        self.address
    }

    /// Returns the signature of the owner.
    pub const fn signature(&self) -> &Signature<N> {
        &self.signature
    }

    /// Verify that the signature is valid for the given transaction ID.
    pub fn verify(&self, transaction_id: N::TransactionID) -> bool {
        self.signature.verify(&self.address, &[*transaction_id])
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use console::{network::Testnet3, types::Field};

    use once_cell::sync::OnceCell;

    type CurrentNetwork = Testnet3;

    pub(crate) fn sample_owner() -> Owner<CurrentNetwork> {
        static INSTANCE: OnceCell<Owner<CurrentNetwork>> = OnceCell::new();
        *INSTANCE.get_or_init(|| {
            // Initialize the RNG.
            let rng = &mut TestRng::default();

            // Initialize a private key.
            let private_key = PrivateKey::<CurrentNetwork>::new(rng).unwrap();

            // Initialize a transaction ID.
            let field: Field<CurrentNetwork> = rng.gen();
            let transaction_id = field.into();

            // Return the owner.
            Owner::new(&private_key, transaction_id, rng).unwrap()
        })
    }

    #[test]
    fn test_verify_owner() {
        // Initialize the RNG.
        let rng = &mut TestRng::default();

        // Initialize a private key.
        let private_key = PrivateKey::<CurrentNetwork>::new(rng).unwrap();

        // Initialize a transaction ID.
        let field: Field<CurrentNetwork> = rng.gen();
        let transaction_id = field.into();

        // Construct the owner.
        let owner = Owner::new(&private_key, transaction_id, rng).unwrap();
        // Ensure that the owner is verified for the given transaction ID.
        assert!(owner.verify(transaction_id));

        // Ensure that the owner is not verified for a different transaction ID.
        let field: Field<CurrentNetwork> = rng.gen();
        let incorrect_transaction_id = field.into();
        assert!(!owner.verify(incorrect_transaction_id));
    }
}
