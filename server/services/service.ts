import * as grpc from "@grpc/grpc-js";
import { MemDb } from "../storage/memdb";
import { toBase64, isBase64 } from "../storage/merkle";
import { Group, VoteOption, VoteResult } from "../interface/gov";

export class Service {
  memDb: MemDb;

  constructor() {
    this.memDb = new MemDb();
  }

  // Implement the service
  commitments(
    call: grpc.ServerUnaryCall<{ group_id: string | number }, any>,
    callback: grpc.sendUnaryData<{ commitments: string[] }>
  ) {
    try {
      const grouplIdRaw = call.request.group_id || 0;
      const grouplId = typeof grouplIdRaw === "string" ? Number(grouplIdRaw) : Number(grouplIdRaw);
      if (!Number.isFinite(grouplId) || grouplId < 0) {
        throw new Error("group id must be a positive integer");
      }
      
      const group = this.memDb?.groups?.get(grouplId) 
      if (!group) {
        throw new Error("group not found")
      }
      let commitments: string[] = group.members.getLeaves();
      
      callback(null, { commitments });
    } catch (e) {
      callback({ code: grpc.status.INVALID_ARGUMENT, message: (e as Error).message } as grpc.ServiceError, null);
    }
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
    callback(null, { proposal_id: proposalId, tally, result });
  }

  createGroup(
    call: grpc.ServerUnaryCall<{ admin?: string, threshold?: string | number, members?: string[] }, { group_id: number }>,
    callback: grpc.sendUnaryData<{ group_id: number }>
  ) {
    try {
      const adminB64 = (call.request.admin ?? "").trim();
      const thresholdRaw = call.request.threshold ?? 0;
      const membersB64 = call.request.members ?? [];

      // validate presence
      if (!adminB64) {
        throw new Error("admin is required (base64 string)");
      }
      if (!Array.isArray(membersB64) || membersB64.length === 0) {
        throw new Error("members must be a non-empty array");
      }

      // validate base64 strings
      if (!isBase64(adminB64)) {
        throw new Error("admin is not valid base64");
      }
      for (let i = 0; i < membersB64.length; i++) {
        if (!isBase64(membersB64[i])) {
          throw new Error(`members[${i}] is not valid base64`);
        }
      }

      // threshold handling (uint64 may arrive as string from proto-loader)
      const threshold = typeof thresholdRaw === "string" ? Number(thresholdRaw) : Number(thresholdRaw);
      if (!Number.isFinite(threshold) || threshold <= 0) {
        throw new Error("threshold must be a positive integer");
      }
      if (threshold > membersB64.length) {
        throw new Error("threshold cannot exceed members length");
      }

      const currentGroupId = this.memDb.groups?.size || 0;
      const group: Group = new Group(adminB64, membersB64, threshold);

      this.memDb.groups.set(currentGroupId, group)
      callback(null, { group_id: currentGroupId });
    }
    catch (e) {
      callback({ code: grpc.status.INVALID_ARGUMENT, message: (e as Error).message } as grpc.ServiceError, null);
    }
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
