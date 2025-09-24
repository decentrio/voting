import { Bytes, fromBase64, SparseMerkleTree } from "../storage/merkle";

export enum VoteResult {
    VOTE_STATUS_PASSED = 0,
    VOTE_STATUS_FAILED = 1,
    VOTE_STATUS_PENDING = 2,
}

export enum VoteOption {
    YES = 0,
    NO = 1,
}

export type Proposal = {
    id: number;
    title: string;
    description: string;
    nullifiers: Map<Uint8Array, boolean>,
    tally: {
        yes: number,
        no: number,
    }
    endTime: Date,
}

export type GroupError = {
    error: string
}

export class Group {
    admin: string;
    proposals: Map<number, Proposal>;
    members: SparseMerkleTree;
    threshold: number;

    constructor(admin: string, members: string[], threshold: number) {
        this.proposals = new Map<number, Proposal>
        this.members = new SparseMerkleTree();
        members.forEach((v, _) => {
            this.members.insertLeaf(fromBase64(v))
        })
        this.admin = admin
        this.threshold = threshold || Math.ceil(members.length * 2 / 3) // default 2/3 members
    }

    addMember(addr: string) {
        if (this.members?.getNextFree() >= this.members?.capacity) {
            throw new Error("reach cap members size")
        }
        const addrBytes = fromBase64(addr);
        const { found } = this.members?.hasLeaf(addrBytes)
        if (found) {
            throw new Error("member already registered")
        }

        this.members?.insertLeaf(addrBytes)
    }

    removeMember(addr: string) {
        const addrBytes = fromBase64(addr);
        const { found, index } = this.members?.hasLeaf(addrBytes)
        if (!found) {
            throw new Error("member not registered yet")
        }

        this.members?.deleteLeaf(index)
    }

    submitProposal(title: string, description: string, endTime: Date): number {
        // TODO: validate proposal
        let currentId = this.proposals ? this.proposals.size : 0
        const proposal: Proposal = {
            id: currentId,
            title,
            description,
            nullifiers: new Map<Uint8Array, boolean>(),
            tally: {
                yes: 0,
                no: 0,
            },
            endTime: endTime || new Date(Date.now() + 2 * 24 * 60 * 60 * 1000) //default 2 days periods
        }
        this.proposals?.set(currentId, proposal)
        return currentId;
    }

    submitVote(proposalId: number, option: VoteOption, nullifier: Bytes) {
        let proposal = this.proposals?.get(proposalId)
        if (!proposal) {
            throw new Error("proposal id not found")
        }
        if (proposal.nullifiers.has(nullifier)) {
            throw new Error("member already voted")
        }
        proposal.nullifiers.set(nullifier, true)
        option == VoteOption.YES ? proposal.tally.yes += 1 : proposal.tally.no += 1

        this.proposals?.set(proposalId, proposal)
    }
}
