use ark_ec::bn::Bn;
use ark_groth16::{Proof, VerifyingKey};
use bitcoin::script::write_scriptint;

use crate::{treepp::*, ExecuteInfo};

/// Define Witness
pub type RawWitness = Vec<Vec<u8>>;

/// Should use u32 version's blake3 hash for fq element
pub use crate::hash::blake3_u32::blake3_var_length;

use super::disprove_execution::RawProof;

/// The depth of a blake3 hash, depending on the defination of `N_DIGEST_U32_LIMBS`
pub(crate) const BLAKE3_HASH_LENGTH: usize =
    crate::hash::blake3_u32::N_DIGEST_U32_LIMBS as usize * 4;
pub type BLAKE3HASH = [u8; BLAKE3_HASH_LENGTH];

/// Commit the original proof, listing all the variable name of original proof.
/// [proof.a, proof.b, proof.c, public_input0, public_input1, public_input2, public_input3]
pub const PROOF_NAMES: [&str; 7] = [
    "F_p4_init",
    "q4",
    "F_p2_init",
    "scalar_1",
    "scalar_2",
    "scalar_3",
    "scalar_4",
];

#[derive(Default)]
pub struct RawProofRecover {
    raw_proof: RawProof,
    proof_a: Option<<Bn<ark_bn254::Config> as ark_ec::pairing::Pairing>::G1Affine>,
    proof_b: Option<<Bn<ark_bn254::Config> as ark_ec::pairing::Pairing>::G2Affine>,
    proof_c: Option<<Bn<ark_bn254::Config> as ark_ec::pairing::Pairing>::G1Affine>,
    proof_public_input: [Option<<ark_bn254::Bn254 as ark_ec::pairing::Pairing>::ScalarField>; 4],
}

impl RawProofRecover {
    pub fn add_witness(&mut self, id: &str, witness: RawWitness) {
        if id == PROOF_NAMES[0] {
        } else if id == PROOF_NAMES[1] {
        } else {
        }
    }

    /// if witness is not enough for generating a raw proof, return none
    pub fn to_raw_proof(&self, vk: VerifyingKey<ark_bn254::Bn254>) -> Option<RawProof> {
        if self.proof_a.is_none() || self.proof_b.is_none() || self.proof_c.is_none() {
            return None;
        }
        let mut inputs_num = 0;
        let mut max_inputs_num = 0;
        for (idx, public_input) in self.proof_public_input.iter().enumerate() {
            if public_input.is_some() {
                inputs_num += 1;
                max_inputs_num = max_inputs_num.max(idx + 1); // start from 1
            }
        }
        if inputs_num == 0 || max_inputs_num != inputs_num {
            return None;
        }

        RawProof {
            proof: Proof::<ark_bn254::Bn254> {
                a: self.proof_a.unwrap(),
                b: self.proof_b.unwrap(),
                c: self.proof_c.unwrap(),
            },
            public: self.proof_public_input.iter().map(|x| x.unwrap()).collect(),
            vk: vk,
        };

        None
    }
}

/// Return witness size of bytes.
pub fn witness_size(witness: &RawWitness) -> usize {
    let mut sum = 0;
    for x in witness {
        sum += x.len();
    }
    sum
}

/// 1 means not equal, 0 means equal.
/// If n is non 0, compare two element of n length is equal or not and left 0 or 1 on stack.
/// If n is 0, return 0.
pub fn not_equal(n: usize) -> Script {
    if n == 0 {
        return script! {OP_FALSE};
    }

    script!(
        for i in 0..n {
            {i + n}
            OP_PICK
            {i + 1}
            OP_PICK
            OP_EQUAL
            OP_TOALTSTACK
        }

        for _ in 0..2*n {
            OP_DROP
        }

        OP_FROMALTSTACK

        for _ in 0..n-1 {
            OP_FROMALTSTACK
            OP_BOOLAND
        }

        OP_NOT
    )
}

/// From witness to hash
pub fn witness_to_array(witness: RawWitness) -> BLAKE3HASH {
    assert_eq!(witness.len(), BLAKE3_HASH_LENGTH);
    let mut res: BLAKE3HASH = [0; BLAKE3_HASH_LENGTH];
    for (idx, byte) in witness.iter().enumerate() {
        if byte.is_empty() {
            res[idx] = 0;
        } else {
            res[idx] = byte[0];
        }
    }
    res
}

/// From hash to witness
pub fn array_to_witness(hash: BLAKE3HASH) -> RawWitness {
    let mut witness = vec![];
    for byte in hash {
        let mut out: [u8; 8] = [0; 8];
        let length = write_scriptint(&mut out, byte as i64);
        witness.push(out[0..length].to_vec());
    }
    witness
}

/// Extract witness from stack.
pub fn extract_witness_from_stack(res: ExecuteInfo) -> RawWitness {
    res.final_stack.0.iter_str().fold(vec![], |mut vector, x| {
        vector.push(x);
        vector
    })
}

/// Compare two elements of n length.
/// If them are not equal, return script's failure directly.
pub fn equalverify(n: usize) -> Script {
    script!(
        for _ in 0..n {
            OP_TOALTSTACK
        }

        for i in 1..n {
            {i}
            OP_ROLL
        }

        for _ in 0..n {
            OP_FROMALTSTACK
            OP_EQUALVERIFY
        }
    )
}
