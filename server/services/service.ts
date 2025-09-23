import * as grpc from "@grpc/grpc-js";
import { MemDb } from "../storage/memdb";
import { toHex } from "../storage/merkle";
import { Group } from "../interface/gov";

export class Service {
  memDb: MemDb;

  constructor() {
    this.memDb = new MemDb();
  }

  // Implement the service
  commitments(
    call: grpc.ServerUnaryCall<{ proposal_id: number, group_id: number }, any>,
    callback: grpc.sendUnaryData<{ commitments: string[] }>
  ) {
    const grouplId = call.request.group_id || 0;
    const proposalId = call.request.proposal_id || 0;
    const leaves = this.memDb?.groups?.get(grouplId)?.proposals.get(proposalId)?.commitments
    let commitments: string[] = [];

    leaves?.getLeaves().forEach((v, _) => {
      commitments.push(toHex(v))
    })

    callback(null, { commitments: commitments });
  }

  createGroup(
    call: grpc.ServerUnaryCall<{ admin: string, members: string[] }, any>,
    callback: grpc.sendUnaryData<{ group_id: number }>
  ) {
    const currentGroupId = this.memDb.groups?.size || 0;
    let members: Map<string, boolean> = new Map<string, boolean>();
    call.request.members.forEach((addr) => {
      members.set(addr, true)
    })
    const group: Group = new Group(call.request.admin, members);
    this.memDb.groups.set(currentGroupId, group)
    callback(null, { group_id: currentGroupId });
  }

  submitProposal(
    call: grpc.ServerUnaryCall<{ group_id: number, title: string, description: string }, any>,
    callback: grpc.sendUnaryData<{ proposal_id: number }>
  ) {
    const grouplId = call.request.group_id || 0;
    const proposalId = this.memDb?.groups?.get(grouplId)?.submitProposal(call.request.title, call.request.description) || 0
    callback(null, { proposal_id: proposalId });
  }

  submitVote(
    call: grpc.ServerUnaryCall<{ group_id: number, proposal_id: number, ciphertext: Uint8Array }, any>,
    callback: grpc.sendUnaryData<{}>
  ) {
    const grouplId = call.request.group_id || 0;
    const proposalId = this.memDb?.groups?.get(grouplId)?.submitVote(call.request.proposal_id, call.request.ciphertext)

    callback(null, { proposal_id: proposalId });
  }
}
