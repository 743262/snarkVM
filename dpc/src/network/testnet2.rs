// Copyright (C) 2019-2021 Aleo Systems Inc.
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

use crate::{
    account::ACCOUNT_ENCRYPTION_AND_SIGNATURE_INPUT,
    posw::PoSW,
    InnerPublicVariables,
    Network,
    NoopProgram,
    OuterPublicVariables,
    PoSWScheme,
    PublicVariables,
};
use snarkvm_algorithms::{
    commitment::{BHPCompressedCommitment, Blake2sCommitment},
    crh::{BHPCompressedCRH, PedersenCompressedCRH},
    encryption::ECIESPoseidonEncryption,
    merkle_tree::{MaskedMerkleTreeParameters, MerkleTreeParameters},
    prelude::*,
    prf::PoseidonPRF,
    signature::AleoSignatureScheme,
    snark::groth16::Groth16,
};
use snarkvm_curves::{
    bls12_377::Bls12_377,
    bw6_761::BW6_761,
    edwards_bls12::{
        EdwardsAffine as EdwardsBls12Affine,
        EdwardsParameters,
        EdwardsProjective as EdwardsBls12Projective,
    },
    edwards_bw6::EdwardsProjective as EdwardsBW6,
    traits::*,
};
use snarkvm_gadgets::{
    algorithms::{
        commitment::{BHPCompressedCommitmentGadget, Blake2sCommitmentGadget},
        crh::{BHPCompressedCRHGadget, PedersenCompressedCRHGadget},
        encryption::ECIESPoseidonEncryptionGadget,
        prf::PoseidonPRFGadget,
        signature::AleoSignatureSchemeGadget,
        snark::Groth16VerifierGadget,
    },
    curves::{bls12_377::PairingGadget, edwards_bls12::EdwardsBls12Gadget, edwards_bw6::EdwardsBW6Gadget},
};
use snarkvm_marlin::{
    constraints::{snark::MarlinSNARK, verifier::MarlinVerificationGadget},
    marlin::MarlinTestnet2Mode,
    FiatShamirAlgebraicSpongeRng,
    PoseidonSponge,
};
use snarkvm_parameters::{testnet2::*, Parameter};
use snarkvm_polycommit::sonic_pc::{sonic_kzg10::SonicKZG10Gadget, SonicKZG10};
use snarkvm_utilities::{FromBytes, ToMinimalBits};

// TODO (howardwu): TEMPORARY - Resolve me.
use snarkvm_marlin::marlin::MarlinTestnet1Mode;

use once_cell::sync::OnceCell;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Testnet2;

// TODO (raychu86): Optimize each of the window sizes in the type declarations below.
#[rustfmt::skip]
impl Network for Testnet2 {
    const NETWORK_ID: u16 = 2u16;

    const NUM_INPUT_RECORDS: usize = 2;
    const NUM_OUTPUT_RECORDS: usize = 2;

    const MEMO_SIZE_IN_BYTES: usize = 64;

    const POSW_PROOF_SIZE_IN_BYTES: usize = 771;
    const POSW_NUM_LEAVES: usize = 4;
    const POSW_TREE_DEPTH: usize = 2;

    type InnerCurve = Bls12_377;
    type InnerScalarField = <Self::InnerCurve as PairingEngine>::Fr;
    
    type OuterCurve = BW6_761;
    type OuterBaseField = <Self::OuterCurve as PairingEngine>::Fq;
    type OuterScalarField = <Self::OuterCurve as PairingEngine>::Fr;

    type ProgramAffineCurve = EdwardsBls12Affine;
    type ProgramAffineCurveGadget = EdwardsBls12Gadget;
    type ProgramProjectiveCurve = EdwardsBls12Projective;
    type ProgramCurveParameters = EdwardsParameters;
    type ProgramBaseField = <Self::ProgramCurveParameters as ModelParameters>::BaseField;
    type ProgramScalarField = <Self::ProgramCurveParameters as ModelParameters>::ScalarField;

    type InnerSNARK = Groth16<Self::InnerCurve, InnerPublicVariables<Testnet2>>;
    type InnerSNARKGadget = Groth16VerifierGadget<Self::InnerCurve, PairingGadget>;

    type OuterSNARK = Groth16<Self::OuterCurve, OuterPublicVariables<Testnet2>>;

    type ProgramSNARK = MarlinSNARK<
        Self::InnerScalarField,
        Self::OuterScalarField,
        SonicKZG10<Self::InnerCurve>,
        FiatShamirAlgebraicSpongeRng<Self::InnerScalarField, Self::OuterScalarField, PoseidonSponge<Self::OuterScalarField>>,
        MarlinTestnet2Mode,
        PublicVariables<Self>,
    >;
    type ProgramSNARKGadget = MarlinVerificationGadget<
        Self::InnerScalarField,
        Self::OuterScalarField,
        SonicKZG10<Self::InnerCurve>,
        SonicKZG10Gadget<Self::InnerCurve, Self::OuterCurve, PairingGadget>,
    >;

    type PoswSNARK = MarlinSNARK<
        Self::InnerScalarField,
        Self::OuterScalarField,
        SonicKZG10<Self::InnerCurve>,
        FiatShamirAlgebraicSpongeRng<Self::InnerScalarField, Self::OuterScalarField, PoseidonSponge<Self::OuterScalarField>>,
        MarlinTestnet1Mode,
        Vec<Self::InnerScalarField>,
    >;
    type PoSWProof = <Self::PoswSNARK as SNARK>::Proof;
    type PoSW = PoSW<Self, 32>;

    type AccountEncryptionScheme = ECIESPoseidonEncryption<Self::ProgramCurveParameters>;
    type AccountEncryptionGadget = ECIESPoseidonEncryptionGadget<Self::ProgramCurveParameters, Self::InnerScalarField>;

    type AccountPRF = PoseidonPRF<Self::ProgramScalarField, 4, false>;
    type AccountSeed = <Self::AccountPRF as PRF>::Seed;
    
    type AccountSignatureScheme = AleoSignatureScheme<Self::ProgramCurveParameters>;
    type AccountSignatureGadget = AleoSignatureSchemeGadget<Self::ProgramCurveParameters, Self::InnerScalarField>;
    type AccountSignaturePublicKey = <Self::AccountSignatureScheme as SignatureScheme>::PublicKey;
    type AccountSignature = <Self::AccountSignatureScheme as SignatureScheme>::Signature;

    type BlockHashCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 117, 63>;
    type BlockHash = <Self::BlockHashCRH as CRH>::Output;

    type BlockHeaderTreeCRH = PedersenCompressedCRH<Self::ProgramProjectiveCurve, 4, 128>;
    type BlockHeaderTreeCRHGadget = PedersenCompressedCRHGadget<Self::ProgramProjectiveCurve, Self::InnerScalarField, Self::ProgramAffineCurveGadget, 4, 128>;
    type BlockHeaderTreeParameters = MaskedMerkleTreeParameters<Self::BlockHeaderTreeCRH, 2>;
    type BlockHeaderRoot = <Self::BlockHeaderTreeCRH as CRH>::Output;

    type CommitmentScheme = BHPCompressedCommitment<Self::ProgramProjectiveCurve, 48, 50>;
    type CommitmentGadget = BHPCompressedCommitmentGadget<Self::ProgramProjectiveCurve, Self::InnerScalarField, Self::ProgramAffineCurveGadget, 48, 50>;
    type Commitment = <Self::CommitmentScheme as CommitmentScheme>::Output;

    type CommitmentsTreeCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 16, 32>;
    type CommitmentsTreeCRHGadget = BHPCompressedCRHGadget<Self::ProgramProjectiveCurve, Self::InnerScalarField, Self::ProgramAffineCurveGadget, 16, 32>;
    type CommitmentsTreeParameters = MerkleTreeParameters<Self::CommitmentsTreeCRH, 32>;
    type CommitmentsRoot = <Self::CommitmentsTreeCRH as CRH>::Output;

    type EncryptedRecordCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 80, 32>;
    type EncryptedRecordCRHGadget = BHPCompressedCRHGadget<Self::ProgramProjectiveCurve, Self::InnerScalarField, Self::ProgramAffineCurveGadget, 80, 32>;
    type EncryptedRecordID = <Self::EncryptedRecordCRH as CRH>::Output;

    type InnerCircuitIDCRH = BHPCompressedCRH<EdwardsBW6, 296, 32>;
    type InnerCircuitIDCRHGadget = BHPCompressedCRHGadget<EdwardsBW6, Self::OuterScalarField, EdwardsBW6Gadget, 296, 32>;
    type InnerCircuitID = <Self::InnerCircuitIDCRH as CRH>::Output;

    type LocalDataCommitmentScheme = BHPCompressedCommitment<Self::ProgramProjectiveCurve, 24, 62>;
    type LocalDataCommitmentGadget = BHPCompressedCommitmentGadget<Self::ProgramProjectiveCurve, Self::InnerScalarField, Self::ProgramAffineCurveGadget, 24, 62>;
    
    type LocalDataCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 16, 32>;
    type LocalDataCRHGadget = BHPCompressedCRHGadget<Self::ProgramProjectiveCurve, Self::InnerScalarField, Self::ProgramAffineCurveGadget, 16, 32>;
    type LocalDataRoot = <Self::LocalDataCRH as CRH>::Output;

    type ProgramCommitmentScheme = Blake2sCommitment;
    type ProgramCommitmentGadget = Blake2sCommitmentGadget;
    type ProgramCommitment = <Self::ProgramCommitmentScheme as CommitmentScheme>::Output;

    type ProgramCircuitIDCRH = BHPCompressedCRH<EdwardsBW6, 237, 16>;
    type ProgramCircuitIDCRHGadget = BHPCompressedCRHGadget<EdwardsBW6, Self::OuterScalarField, EdwardsBW6Gadget, 237, 16>;
    type ProgramCircuitID = <Self::ProgramCircuitIDCRH as CRH>::Output;

    type ProgramCircuitIDTreeCRH = BHPCompressedCRH<EdwardsBW6, 48, 16>;
    type ProgramCircuitIDTreeCRHGadget = BHPCompressedCRHGadget<EdwardsBW6, Self::OuterScalarField, EdwardsBW6Gadget, 48, 16>;
    type ProgramCircuitTreeParameters = MerkleTreeParameters<Self::ProgramCircuitIDTreeCRH, 8>;
    type ProgramID = <Self::ProgramCircuitIDTreeCRH as CRH>::Output;

    type SerialNumberNonceCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 32, 63>;
    type SerialNumberNonceCRHGadget = BHPCompressedCRHGadget<Self::ProgramProjectiveCurve, Self::InnerScalarField, Self::ProgramAffineCurveGadget, 32, 63>;
    type SerialNumberNonce = <Self::SerialNumberNonceCRH as CRH>::Output;

    type SerialNumberPRF = PoseidonPRF<Self::InnerScalarField, 4, false>;
    type SerialNumberPRFGadget = PoseidonPRFGadget<Self::InnerScalarField, 4, false>;
    type SerialNumber = <Self::SerialNumberPRF as PRF>::Output;

    type SerialNumbersTreeCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 16, 32>;
    type SerialNumbersTreeParameters = MerkleTreeParameters<Self::SerialNumbersTreeCRH, 32>;
    type SerialNumbersRoot = <Self::SerialNumbersTreeCRH as CRH>::Output;

    type TransactionIDCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 26, 62>;
    type TransactionID = <Self::TransactionIDCRH as CRH>::Output;

    type TransactionsTreeCRH = BHPCompressedCRH<Self::ProgramProjectiveCurve, 16, 32>;
    type TransactionsTreeParameters = MerkleTreeParameters<Self::TransactionsTreeCRH, 16>;
    type TransactionsRoot = <Self::TransactionsTreeCRH as CRH>::Output;

    dpc_setup!{Testnet2, account_encryption_scheme, AccountEncryptionScheme, ACCOUNT_ENCRYPTION_AND_SIGNATURE_INPUT}
    dpc_setup!{Testnet2, account_signature_scheme, AccountSignatureScheme, ACCOUNT_ENCRYPTION_AND_SIGNATURE_INPUT}
    dpc_setup!{Testnet2, block_hash_crh, BlockHashCRH, "AleoBlockHashCRH0"}
    dpc_setup!{Testnet2, block_header_tree_crh, BlockHeaderTreeCRH, "AleoBlockHeaderTreeCRH0"}
    dpc_setup!{Testnet2, commitment_scheme, CommitmentScheme, "AleoCommitmentScheme0"}
    dpc_setup!{Testnet2, commitments_tree_crh, CommitmentsTreeCRH, "AleoCommitmentsTreeCRH0"}
    dpc_merkle!{Testnet2, commitments_tree_parameters, CommitmentsTreeParameters, commitments_tree_crh}
    dpc_setup!{Testnet2, encrypted_record_crh, EncryptedRecordCRH, "AleoEncryptedRecordCRH0"}
    dpc_setup!{Testnet2, inner_circuit_id_crh, InnerCircuitIDCRH, "AleoInnerCircuitIDCRH0"}
    dpc_setup!{Testnet2, local_data_commitment_scheme, LocalDataCommitmentScheme, "AleoLocalDataCommitmentScheme0"}
    dpc_setup!{Testnet2, local_data_crh, LocalDataCRH, "AleoLocalDataCRH0"}
    dpc_setup!{Testnet2, program_commitment_scheme, ProgramCommitmentScheme, "AleoProgramCommitmentScheme0"}
    dpc_setup!{Testnet2, program_circuit_id_crh, ProgramCircuitIDCRH, "AleoProgramCircuitIDCRH0"}
    dpc_setup!{Testnet2, program_circuit_id_tree_crh, ProgramCircuitIDTreeCRH, "AleoProgramCircuitIDTreeCRH0"}
    dpc_merkle!{Testnet2, program_circuit_tree_parameters, ProgramCircuitTreeParameters, program_circuit_id_tree_crh}
    dpc_setup!{Testnet2, serial_number_nonce_crh, SerialNumberNonceCRH, "AleoSerialNumberNonceCRH0"}
    dpc_setup!{Testnet2, serial_numbers_tree_crh, SerialNumbersTreeCRH, "AleoSerialNumbersTreeCRH0"}
    dpc_merkle!{Testnet2, serial_numbers_tree_parameters, SerialNumbersTreeParameters, serial_numbers_tree_crh}
    dpc_setup!{Testnet2, transaction_id_crh, TransactionIDCRH, "AleoTransactionIDCRH0"}
    dpc_setup!{Testnet2, transactions_tree_crh, TransactionsTreeCRH, "AleoTransactionsTreeCRH0"}
    dpc_merkle!{Testnet2, transactions_tree_parameters, TransactionsTreeParameters, transactions_tree_crh}

    dpc_snark_setup!{Testnet2, inner_circuit_proving_key, InnerSNARK, ProvingKey, InnerSNARKPKParameters, "inner circuit proving key"}
    dpc_snark_setup!{Testnet2, inner_circuit_verifying_key, InnerSNARK, VerifyingKey, InnerSNARKVKParameters, "inner circuit verifying key"}

    dpc_snark_setup!{Testnet2, outer_circuit_proving_key, OuterSNARK, ProvingKey, OuterSNARKPKParameters, "outer circuit proving key"}
    dpc_snark_setup!{Testnet2, outer_circuit_verifying_key, OuterSNARK, VerifyingKey, OuterSNARKVKParameters, "outer circuit verifying key"}

    dpc_snark_setup!{Testnet2, noop_circuit_proving_key, ProgramSNARK, ProvingKey, NoopProgramSNARKPKParameters, "noop circuit proving key"}
    dpc_snark_setup!{Testnet2, noop_circuit_verifying_key, ProgramSNARK, VerifyingKey, NoopProgramSNARKVKParameters, "noop circuit verifying key"}

    fn inner_circuit_id() -> &'static Self::InnerCircuitID {
        static INNER_CIRCUIT_ID: OnceCell<<Testnet2 as Network>::InnerCircuitID> = OnceCell::new();
        INNER_CIRCUIT_ID.get_or_init(|| Self::inner_circuit_id_crh()
            .hash_bits(&Self::inner_circuit_verifying_key().to_minimal_bits())
            .expect("Failed to hash inner circuit verifying key elements"))
    }
    
    fn noop_program() -> &'static NoopProgram<Self> {
        static NOOP_PROGRAM: OnceCell<NoopProgram<Testnet2>> = OnceCell::new();
        NOOP_PROGRAM.get_or_init(|| NoopProgram::<Testnet2>::load().expect("Failed to fetch the noop program"))
    }

    fn noop_circuit_id() -> &'static Self::ProgramCircuitID {
        static NOOP_CIRCUIT_ID: OnceCell<<Testnet2 as Network>::ProgramCircuitID> = OnceCell::new();
        NOOP_CIRCUIT_ID.get_or_init(|| Self::program_circuit_id(Self::noop_circuit_verifying_key()).expect("Failed to hash noop circuit verifying key"))
    }

    fn posw() -> &'static Self::PoSW {
        static POSW: OnceCell<<Testnet2 as Network>::PoSW> = OnceCell::new();
        POSW.get_or_init(|| <Self::PoSW as PoSWScheme<Self>>::load(true).expect("Failed to load PoSW"))
    }
    
    /// Returns the program SRS for Aleo applications.
    fn program_srs<R: Rng + CryptoRng>(_rng: &mut R) -> Rc<RefCell<SRS<R, <Self::ProgramSNARK as SNARK>::UniversalSetupParameters>>> {
        static UNIVERSAL_SRS: OnceCell<<<Testnet2 as Network>::ProgramSNARK as SNARK>::UniversalSetupParameters> = OnceCell::new();
        let universal_srs = UNIVERSAL_SRS.get_or_init(|| <Self::ProgramSNARK as SNARK>::UniversalSetupParameters::from_bytes_le(
            &UniversalSRSParameters::load_bytes().expect("Failed to load universal SRS bytes"),
        ).unwrap());
        Rc::new(RefCell::new(SRS::<_, _>::Universal(universal_srs)))
    }
    
    fn block_header_tree_parameters() -> &'static Self::BlockHeaderTreeParameters {
        static MASKED_MERKLE_TREE_PARAMETERS: OnceCell<<Testnet2 as Network>::BlockHeaderTreeParameters> = OnceCell::new();
        MASKED_MERKLE_TREE_PARAMETERS.get_or_init(|| Self::BlockHeaderTreeParameters::setup("MerkleTreeParameters"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inner_circuit_sanity_check() {
        // Verify the inner circuit verifying key matches the one derived from the inner circuit proving key.
        assert_eq!(
            Testnet2::inner_circuit_verifying_key(),
            &Testnet2::inner_circuit_proving_key().vk,
            "The inner circuit verifying key does not correspond to the inner circuit proving key"
        );
    }

    #[test]
    fn test_inner_circuit_id_derivation() {
        // Verify the inner circuit ID matches the one derived from the inner circuit verifying key.
        assert_eq!(
            Testnet2::inner_circuit_id(),
            &Testnet2::inner_circuit_id_crh()
                .hash_bits(&Testnet2::inner_circuit_verifying_key().to_minimal_bits())
                .expect("Failed to hash inner circuit ID"),
            "The inner circuit ID does not correspond to the inner circuit verifying key"
        );
    }

    #[test]
    fn test_outer_circuit_sanity_check() {
        // Verify the outer circuit verifying key matches the one derived from the outer circuit proving key.
        assert_eq!(
            Testnet2::outer_circuit_verifying_key(),
            &Testnet2::outer_circuit_proving_key().vk,
            "The outer circuit verifying key does not correspond to the outer circuit proving key"
        );
    }

    #[test]
    fn test_posw_tree_sanity_check() {
        // Verify the PoSW tree depth matches the declared depth.
        assert_eq!(
            Testnet2::POSW_TREE_DEPTH,
            <<Testnet2 as Network>::BlockHeaderTreeParameters as MerkleParameters>::DEPTH
        );

        // Verify the number of leaves corresponds to the correct tree depth.
        let num_leaves = 2u32.pow(Testnet2::POSW_TREE_DEPTH as u32) as usize;
        assert_eq!(Testnet2::POSW_NUM_LEAVES, num_leaves);
    }
}
