pub mod tree;
pub mod voter;
pub mod proposal;

pub type ConstraintF = ark_bls12_381::Fr;

use ark_snark::SNARK;
use ark_std::rand::{CryptoRng, RngCore};

use ark_std::marker::PhantomData;

#[derive(Clone)]
pub struct Proposal<S> {
    _snark: PhantomData<S>, // WIP
    parameters: proposal::Parameters,
}

impl<S: SNARK<ConstraintF>> Proposal<S> {
    pub fn new(parameters: proposal::Parameters) -> Self {
        Self {
            _snark: PhantomData,
            parameters,
        }
    }
    pub fn new_proposal_id(&self, pid: u16) -> voter::ProposalId {
        ConstraintF::from(pid)
    }
    pub fn new_voter<R: CryptoRng + RngCore>(&self, rng: &mut R) -> voter::Voter {
        voter::Voter::new(&self.parameters.leaf_crh_params, rng)
    }
    pub fn new_vote(&self, v: u8) -> voter::Vote {
        ConstraintF::from(v)
    }
    pub fn new_tree(
        &self,
        // n_voters: usize,
        height: usize
    ) -> Result<tree::Tree, tree::Error> {
        // let height = ark_std::log2(n_voters) as usize + 1;
        tree::Tree::blank(
            &self.parameters.leaf_crh_params,
            &self.parameters.two_to_one_crh_params,
            height,
        )
    }
    pub fn new_circuit_instance(
        self,
        root: tree::Root,
        proposal_id: voter::ProposalId,
        nullifier: voter::Nullifier,
        vote: voter::Vote,
        sk: voter::SecretKey,
        proof: tree::Proof,
    ) -> proposal::ProposalCircuit {
        proposal::ProposalCircuit {
            parameters: proposal::Parameters {
                leaf_crh_params: self.parameters.leaf_crh_params,
                two_to_one_crh_params: self.parameters.two_to_one_crh_params,
            },
            root: Some(root),
            proposal_id: Some(proposal_id),
            nullifier: Some(nullifier),
            vote: Some(vote),
            sk: Some(sk),
            proof: Some(proof),
        }
    }
}

impl<S: SNARK<ConstraintF>> Proposal<S> {
    pub fn circuit_setup<R: CryptoRng + RngCore>(
        rng: &mut R,
        circuit: proposal::ProposalCircuit,
    ) -> Result<(S::ProvingKey, S::VerifyingKey), S::Error> {
        S::circuit_specific_setup(circuit, rng)
    }

    pub fn prove<R: CryptoRng + RngCore>(
        rng: &mut R,
        pk: S::ProvingKey,
        circuit: proposal::ProposalCircuit,
    ) -> Result<S::Proof, S::Error> {
        S::prove(&pk, circuit, rng)
    }

    pub fn verify(
        vk: S::VerifyingKey,
        proof: S::Proof,
        public_input: Vec<ConstraintF>,
    ) -> Result<bool, S::Error> {
        S::verify(&vk, &public_input, &proof)
    }
}
