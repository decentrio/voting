import { Bytes, SparseMerkleTree } from "../storage/merkle";

export type Proposal = {
    id: number;
    title: string;
    description: string;
    commitments: SparseMerkleTree;
}

export type GroupError = {
    error: string
}

export class Group {
    admin: string;
    proposals: Map<number, Proposal>;
    members: Map<string, boolean>;
    
    constructor(admin: string, members: Map<string, boolean>) {
        this.proposals = new Map<number, Proposal>
        this.members = members
        this.admin = admin
    }

    addMember(addr: string) {
        if (this.members?.has(addr) && this.members?.get(addr)) {
            throw new Error("member already registered")
        }

        this.members?.set(addr, true)
    }

    removeMember(addr: string) {
        if (!this.members?.has(addr) || !this.members?.get(addr)) {
            throw new Error("member not registered yet")
        }

        this.members?.delete(addr)
    }

    submitProposal(title: string, description: string): number {
        // TODO: validate proposal
        let currentId = this.proposals ? this.proposals.size : 0
        const proposal: Proposal = {
            id: currentId,
            title,
            description,
            commitments: new SparseMerkleTree({
                depth: Math.ceil(Math.log2(this.members.size))
            })
        }
        this.proposals?.set(currentId, proposal)
        return currentId;
    }

    submitVote(proposalId: number, ciphertext: Bytes) {
        let proposal = this.proposals?.get(proposalId) 
        if (!proposal) {
            throw new Error("proposal id not found")
        }
        proposal.commitments.insertLeaf(ciphertext)

        this.proposals?.set(proposalId, proposal)
    }
}
