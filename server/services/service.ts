import * as grpc from "@grpc/grpc-js";
import { MemDb } from "../storage/memdb";
import { toHex } from "../storage/merkle";
import { Group, VoteOption, VoteResult } from "../interface/gov";
import "dotenv/config";
import { readFileSync, writeFileSync } from "fs";
import { zkVerifySession, Library, CurveType, ZkVerifyEvents } from "zkverifyjs";

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
    callback(null, { proposal_id: proposalId, tally, result });
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

  async submitVote(
    call: grpc.ServerUnaryCall<{ group_id: number, proposal_id: number, option: VoteOption, nullifier: Uint8Array }, any>,
    callback: grpc.sendUnaryData<{}>
  ) {
    const grouplId = call.request.group_id || 0;
    const proposalId = this.memDb?.groups?.get(grouplId)?.submitVote(
      call.request.proposal_id,
      call.request.option,
      call.request.nullifier
    )

    const key = JSON.parse(readFileSync(`${__dirname}/../../data/vkey.json`).toString());
    console.log(key)
    const proof = JSON.parse(readFileSync(`${__dirname}/../../data/proof.json`).toString());
    const publicInputs = JSON.parse(readFileSync(`${__dirname}/../../data/public_inputs.json`).toString());
    const session = await zkVerifySession.start().Volta().withAccount(seedPhrase);

    if (this.submitVkey === false) {
      const convertedVkey = convert(key);
      // console.log("convertedVkey: ", convertedVkey)
      console.log("Registering verification key...");
      const { events: regevent } = await session.registerVerificationKey().groth16({ library: Library.snarkjs, curve: CurveType.bls12381 }).execute(convertedVkey);
      // console.log(regevent)
      regevent.on(ZkVerifyEvents.Finalized, (eventData) => {
        console.log('Registration finalized:', eventData);
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
          console.log("New aggregation receipt:", eventData);
          if (aggregationId == parseInt(eventData.data.aggregationId.replace(/,/g, ''))) {
            let statementpath = await retryUntilOk(async () => {
              return session.getAggregateStatementPath(
                eventData.blockHash,
                parseInt(eventData.data.domainId),
                parseInt(eventData.data.aggregationId.replace(/,/g, '')),
                statement
              );
            });
            console.log("Statement path:", statementpath);
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

    console.log(proof)
    console.log(publicInputs)

    try {
      const { events } = await session.verify()
        .groth16({ library: Library.snarkjs, curve: CurveType.bls12381, })
        .withRegisteredVk()
        .execute({
          proofData: {
            vk: vkey.hash,
            proof: convert(proof),
            publicSignals: convert(publicInputs)
          }, domainId: 0
        });

      events.on(ZkVerifyEvents.IncludedInBlock, (eventData) => {
        console.log("Included in block", eventData);
        statement = eventData.statement;
        aggregationId = eventData.aggregationId;
      })

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
