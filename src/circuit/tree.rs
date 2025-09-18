use crate::circuit::ConstraintF;
use crate::circuit::voter::{Voter, VoterVar};
use ark_crypto_primitives::crh::{
    injective_map::{
        constraints::{PedersenCRHCompressorGadget, TECompressorGadget},
        PedersenCRHCompressor, TECompressor,
    },
    pedersen, CRHGadget, TwoToOneCRH, TwoToOneCRHGadget, CRH,
};
use ark_crypto_primitives::{
    merkle_tree::{constraints::PathVar, Config, MerkleTree, Path},
    Error as MTError,
};
use ark_ed_on_bls12_381::{constraints::EdwardsVar, EdwardsProjective};
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub type Error = MTError;

pub type LeafHash = PedersenCRHCompressor<EdwardsProjective, TECompressor, LeafWindow>;
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct LeafWindow;

pub type TwoToOneHash = PedersenCRHCompressor<EdwardsProjective, TECompressor, TwoToOneWindow>;
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TwoToOneWindow;

// `WINDOW_SIZE * NUM_WINDOWS` = 2 * 256 bits = enough for hashing two outputs.
impl pedersen::Window for LeafWindow {
    const WINDOW_SIZE: usize = 4;
    const NUM_WINDOWS: usize = 144;
}
// `WINDOW_SIZE * NUM_WINDOWS` = 2 * 256 bits = enough for hashing two outputs.
impl pedersen::Window for TwoToOneWindow {
    const WINDOW_SIZE: usize = 4;
    const NUM_WINDOWS: usize = 128;
}

#[derive(Clone)]
pub struct TreeConfig;
impl Config for TreeConfig {
    type LeafHash = LeafHash; // to hash leaves
    type TwoToOneHash = TwoToOneHash; // to hash pairs of internal nodes
}


pub type Tree = MerkleTree<TreeConfig>;
pub type Root = <TwoToOneHash as TwoToOneCRH>::Output;
pub type Proof = Path<TreeConfig>;


//////////////
// constraints
pub type TwoToOneHashGadget = PedersenCRHCompressorGadget<
    EdwardsProjective,
    TECompressor,
    TwoToOneWindow,
    EdwardsVar,
    TECompressorGadget,
>;

pub type LeafHashGadget = PedersenCRHCompressorGadget<
    EdwardsProjective,
    TECompressor,
    LeafWindow,
    EdwardsVar,
    TECompressorGadget,
>;
pub type LeafHashParamsVar = <LeafHashGadget as CRHGadget<LeafHash, ConstraintF>>::ParametersVar;
pub type TwoToOneHashParamsVar =
    <TwoToOneHashGadget as TwoToOneCRHGadget<TwoToOneHash, ConstraintF>>::ParametersVar;

pub type RootVar = <TwoToOneHashGadget as TwoToOneCRHGadget<TwoToOneHash, ConstraintF>>::OutputVar;
pub type ProofVar = PathVar<TreeConfig, LeafHashGadget, TwoToOneHashGadget, ConstraintF>;


pub struct TreeVerification {
    // parameters
    pub leaf_crh_params: <LeafHash as CRH>::Parameters,
    pub two_to_one_crh_params: <TwoToOneHash as TwoToOneCRH>::Parameters,

    // public inputs
    pub root: Root,

    // private inputs
    pub leaf: Voter,
    pub proof: Option<Proof>,
}

impl ConstraintSynthesizer<ConstraintF> for TreeVerification {
    fn generate_constraints(
        self,
        cs: ConstraintSystemRef<ConstraintF>,
    ) -> Result<(), SynthesisError> {
        // allocate the public inputs
        let root = RootVar::new_input(ark_relations::ns!(cs, "root_var"), || Ok(&self.root))?;

        let leaf = VoterVar::new_input(ark_relations::ns!(cs, "leaf_var"), || Ok(&self.leaf))?;

        // allocate the public parameters as constants:
        let leaf_crh_params = LeafHashParamsVar::new_constant(cs.clone(), &self.leaf_crh_params)?;
        let two_to_one_crh_params =
            TwoToOneHashParamsVar::new_constant(cs.clone(), &self.two_to_one_crh_params)?;

        // allocate the merkle proof as a private witness variable:
        let proof = ProofVar::new_witness(ark_relations::ns!(cs, "proof_var"), || {
            Ok(self.proof.as_ref().unwrap())
        })?;

        let is_member = proof.verify_membership(
            &leaf_crh_params,
            &two_to_one_crh_params,
            &root,
            &leaf.to_bytes_le().as_slice(),
        )?;

        is_member.enforce_equal(&Boolean::TRUE)?;

        Ok(())
    }
}
