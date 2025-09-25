import * as grpc from "@grpc/grpc-js";
import { MemDb } from "../storage/memdb";
import { toBase64, isBase64 } from "../storage/merkle";
import { Group, VoteOption, VoteResult } from "../interface/gov";
import "dotenv/config";
import { readFileSync, writeFileSync } from "fs";
import { zkVerifySession, Library, CurveType, ZkVerifyEvents, VerifyTransactionInfo } from "zkverifyjs";

const seedPhrase: string = process.env.SEED_PHRASE!;
export class Service {
  memDb: MemDb;
  submitVkey: boolean;
  constructor() {
    this.memDb = new MemDb();
    this.submitVkey = false;
  }

  // Implement the service
  commitments(
    call: grpc.ServerUnaryCall<{ group_id: string | number }, any>,
    callback: grpc.sendUnaryData<{ commitments: string[] }>
  ) {
    try {
      const grouplIdRaw = call.request.group_id || 0;
      const grouplId = typeof grouplIdRaw === "string" ? Number(grouplIdRaw) : grouplIdRaw;
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
      callback({ code: grpc.status.ABORTED, message: (e as Error).message } as grpc.ServiceError, null);
    }
  }

  VoteResult(
    call: grpc.ServerUnaryCall<{ proposal_id: string | number, group_id: string | number }, any>,
    callback: grpc.sendUnaryData<{ proposal_id: number, tally: { yes: number, no: number }, result: VoteResult }>
  ) {
    try {
      const grouplIdRaw = call.request.group_id || 0;
      const grouplId = typeof grouplIdRaw === "string" ? Number(grouplIdRaw) : grouplIdRaw;
      const proposalIdRaw = call.request.proposal_id || 0;
      const proposalId = typeof proposalIdRaw === "string" ? Number(proposalIdRaw) : proposalIdRaw;

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
        throw new Error("tally not found")
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
    } catch (e) {
      callback({ code: grpc.status.ABORTED, message: (e as Error).message } as grpc.ServiceError, null);
    }
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
      callback({ code: grpc.status.ABORTED, message: (e as Error).message } as grpc.ServiceError, null);
    }
  }

  submitProposal(
    call: grpc.ServerUnaryCall<{ group_id: string | number, title: string, description: string, end_time: number }, any>,
    callback: grpc.sendUnaryData<{ proposal_id: number }>
  ) {
    try {
      if (!call.request.group_id) {
        return callback({ code: grpc.status.INVALID_ARGUMENT, message: "need to provide proposal id" } as grpc.ServiceError, null);
      }
      const grouplId = typeof call.request.group_id === "string" ? Number(call.request.group_id) : call.request.group_id;
      const proposalId = this.memDb?.groups?.get(grouplId)?.submitProposal(
        call.request.title,
        call.request.description,
        new Date(call.request.end_time),
      ) || 0
      callback(null, { proposal_id: proposalId });
    } catch (e) {
      callback({ code: grpc.status.ABORTED, message: (e as Error).message } as grpc.ServiceError, null);
    }
  }

  async submitVote(
    call: grpc.ServerUnaryCall<{ group_id: string | number, proposal_id: string | number, option: VoteOption, nullifier: Uint8Array }, any>,
    callback: grpc.sendUnaryData<{}>
  ) {
    if (!call.request.group_id) {
      return callback({ code: grpc.status.INVALID_ARGUMENT, message: "need to provide proposal id" } as grpc.ServiceError, null);
    }
    const grouplId = typeof call.request.group_id === "string" ? Number(call.request.group_id) : call.request.group_id;
    if (!call.request.proposal_id) {
      return callback({ code: grpc.status.INVALID_ARGUMENT, message: "need to provide proposal id" } as grpc.ServiceError, null);
    }
    const proposalId = typeof call.request.proposal_id === "string" ? Number(call.request.proposal_id) : call.request.proposal_id;
    this.memDb?.groups?.get(grouplId)?.submitVote(
      proposalId,
      call.request.option,
      call.request.nullifier
    )

    const key = JSON.parse(readFileSync(`${__dirname}/../../data/vkey.json`).toString());
    const proof = JSON.parse(readFileSync(`${__dirname}/../../data/proof.json`).toString());
    const publicInputs = JSON.parse(readFileSync(`${__dirname}/../../data/public_inputs.json`).toString());
    const session = await zkVerifySession.start().Volta().withAccount(seedPhrase);

    if (this.submitVkey === false) {
      const convertedVkey = convert(key);
      // console.log("convertedVkey: ", convertedVkey)
      const { events: regevent } = await session.registerVerificationKey().groth16({ library: Library.snarkjs, curve: CurveType.bls12381 }).execute(convertedVkey);
      // console.log(regevent)
      regevent.on(ZkVerifyEvents.Finalized, (eventData: { statementHash: any; }) => {
        writeFileSync(`${__dirname}/../../data/vkey_hash.json`, JSON.stringify({ "hash": eventData.statementHash }, null, 2));
        return eventData.statementHash
      });

      this.submitVkey = true;
    }

    let vkey;
    let timer = 0;
    let count = 0;
    while (!vkey) {
      try {
        count += 1;
        await sleep(timer);
        vkey = JSON.parse(readFileSync(`${__dirname}/../../data/vkey_hash.json`).toString());
      } catch (e) {
        timer += 2000;
        console.log("Waiting for vkey registration to complete...", count);
      }
    }
    let statement: string, aggregationId: number;
    session.subscribe([
      {
        event: ZkVerifyEvents.NewAggregationReceipt,
        callback: async (eventData: any) => {
          if (aggregationId == parseInt(eventData.data.aggregationId.replace(/,/g, ''))) {
            let statementpath = await retryUntilOk(async () => {
              return session.getAggregateStatementPath(
                eventData.blockHash,
                parseInt(eventData.data.domainId),
                parseInt(eventData.data.aggregationId.replace(/,/g, '')),
                statement
              );
            });
            const statementproof = {
              ...statementpath,
              domainId: parseInt(eventData.data.domainId),
              aggregationId: parseInt(eventData.data.aggregationId.replace(/,/g, '')),
            };
            writeFileSync(`${__dirname}/../../data/aggregation.json`, JSON.stringify(statementproof));
          }
        },
        options: { domainId: 0 },
      },
    ]);

    const { events, transactionResult } = await session.verify()
      .groth16({ library: Library.snarkjs, curve: CurveType.bls12381, })
      .withRegisteredVk()
      .execute({
        proofData: {
          vk: vkey.hash,
          proof: convert(proof),
          publicSignals: convert(publicInputs)
        }, domainId: 0
      });

    events.on(ZkVerifyEvents.IncludedInBlock, (eventData: { statement: string; aggregationId: number; }) => {
      console.log("Included in block", eventData);
      statement = eventData.statement;
      aggregationId = eventData.aggregationId;
    })

    // Handle errors during the transaction process
    events.on('error', (error) => {
      console.error('An error occurred during the transaction:', error);
      throw error
    });

    try {
      await transactionResult;

      callback(null, { proposal_id: proposalId });
    } catch (e) {
      console.log("Error during verification: ", e);
      callback(
        {
          code: grpc.status.INTERNAL,
          message: "verification failed",
        } as grpc.ServiceError,
        null
      )
    } finally {
      // Close the session when done
      await session.close();
    }
  }
}

function hexToDecimal(hex: string): string {
  // Ensure it works for big numbers
  return BigInt("0x" + hex).toString(10);
}

function convert(obj: any): any {
  if (typeof obj === "string") {
    // if it's hex (only [0-9A-F]), convert
    if (/^[0-9A-F]+$/i.test(obj)) {
      return hexToDecimal(obj);
    }
    return obj;
  } else if (Array.isArray(obj)) {
    return obj.map((v) => convert(v));
  } else if (typeof obj === "object" && obj !== null) {
    const res: any = {};
    for (const k in obj) {
      res[k] = convert(obj[k]);
    }
    return res;
  }
  return obj;
}

function sleep(ms: number) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function retryUntilOk<T>(fn: () => Promise<T>, delayMs = 1000): Promise<T> {
  while (true) {
    try {
      return await fn(); // if ok, return result
    } catch (err) {
      console.error("Error:", err);
      console.log(`Retrying in ${delayMs / 1000}s...`);
      await sleep(delayMs);
    }
  }
}
