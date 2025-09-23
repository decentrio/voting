import * as grpc from "@grpc/grpc-js";
import { MemDb } from "../storage/memdb";
import { toHex } from "../storage/merkle";
import { Group, VoteOption, VoteResult } from "../interface/gov";

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
    const members = this.memDb?.groups?.get(grouplId)?.members
    let commitments: string[] = [];

    members?.getLeaves().forEach((v, _) => {
      commitments.push(toHex(v))
    })

    callback(null, { commitments: commitments });
  }

  VoteResult(
    call: grpc.ServerUnaryCall<{ proposal_id: number, group_id: number }, any>,
    callback: grpc.sendUnaryData<{ proposal_id: number, tally: { yes: number, no: number }, result: VoteResult }>
  ) {
    const grouplId = call.request.group_id || 0;
    const proposalId = call.request.proposal_id || 0;
    const group = this.memDb?.groups?.get(grouplId);
    if (!group) {
      return callback(
        {
          code: grpc.status.NOT_FOUND,
          message: "group not found",
        } as grpc.ServiceError,
        null
      )
    }

    const proposal = group.proposals?.get(proposalId);
    if (!proposal) {
      return callback(
        {
          code: grpc.status.NOT_FOUND,
          message: "proposal not found",
        } as grpc.ServiceError,
        null
      )
    }

    const tally = proposal.tally;
    if (!tally) {
      return callback(
        {
          code: grpc.status.NOT_FOUND,
          message: "tally not found",
        } as grpc.ServiceError,
        null
      )
    }
    let result = 0
    if (proposal.endTime.getTime() > Date.now()) {
      result = VoteResult.VOTE_STATUS_PENDING
    } else if (proposal.tally.yes >= group.threshold) {
      result = VoteResult.VOTE_STATUS_PASSED
    } else {
      result = VoteResult.VOTE_STATUS_FAILED
    }
    callback(null, { proposal_id: proposalId, tally, result});
  }

  createGroup(
    call: grpc.ServerUnaryCall<{ admin: string, threshold: number, members: string[] }, any>,
    callback: grpc.sendUnaryData<{ group_id: number }>
  ) {
    const currentGroupId = this.memDb.groups?.size || 0;
    const group: Group = new Group(call.request.admin, call.request.members, call.request.threshold);
    this.memDb.groups.set(currentGroupId, group)
    callback(null, { group_id: currentGroupId });
  }

  submitProposal(
    call: grpc.ServerUnaryCall<{ group_id: number, title: string, description: string, end_time: number }, any>,
    callback: grpc.sendUnaryData<{ proposal_id: number }>
  ) {
    const grouplId = call.request.group_id || 0;
    const proposalId = this.memDb?.groups?.get(grouplId)?.submitProposal(
      call.request.title, 
      call.request.description,
      new Date(call.request.end_time),
    ) || 0
    callback(null, { proposal_id: proposalId });
  }

  submitVote(
    call: grpc.ServerUnaryCall<{ group_id: number, proposal_id: number, option: VoteOption, nullifier: Uint8Array }, any>,
    callback: grpc.sendUnaryData<{}>
  ) {
    const grouplId = call.request.group_id || 0;
    const proposalId = this.memDb?.groups?.get(grouplId)?.submitVote(
      call.request.proposal_id,
      call.request.option,
      call.request.nullifier
    )

    callback(null, { proposal_id: proposalId });
  }
}
