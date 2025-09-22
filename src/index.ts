import "dotenv/config";
import { readFileSync, writeFileSync } from "fs";

import { zkVerifySession, Library, CurveType, ZkVerifyEvents } from "zkverifyjs";

// const key = JSON.parse(readFileSync("./data/vkey.json").toString());
import key from "../data/vkey.json";
// console.log(key);

const proof = JSON.parse(readFileSync("./data/proof.json").toString());
const publicInputs = JSON.parse(readFileSync("./data/public_inputs.json").toString());
async function main() {

  // const convertedVkey = convert(key);
  // writeFileSync("output.json", JSON.stringify(converted, null, 2), "utf8");
  // console.log("✅ Converted JSON written to output.json");


  const seedPhrase: string = process.env.SEED_PHRASE!;
  const session = await zkVerifySession.start().Volta().withAccount(seedPhrase);

  // console.log("Registering verification key...");
  // const { events: regevent } = await session.registerVerificationKey().groth16({ library: Library.snarkjs, curve: CurveType.bls12381 }).execute(convertedVkey);

  // regevent.on(ZkVerifyEvents.Finalized, (eventData) => {
  //   console.log('Registration finalized:', eventData);
  //   writeFileSync("./vkey_hash.json", JSON.stringify({ "hash": eventData.statementHash }, null, 2));
  //   return eventData.statementHash
  // });


  const vkey = JSON.parse(readFileSync("./vkey_hash.json").toString());

  let statement: string, aggregationId: number;
  session.subscribe([
  {
    event: ZkVerifyEvents.NewAggregationReceipt,
    callback: async (eventData: any) => {
      console.log("New aggregation receipt:", eventData);
      if(aggregationId == parseInt(eventData.data.aggregationId.replace(/,/g, ''))){
        let statementpath = await session.getAggregateStatementPath(
          eventData.blockHash,
          parseInt(eventData.data.domainId),
          parseInt(eventData.data.aggregationId.replace(/,/g, '')),
          statement
        );
        console.log("Statement path:", statementpath);
        const statementproof = {
          ...statementpath,
          domainId: parseInt(eventData.data.domainId),
          aggregationId: parseInt(eventData.data.aggregationId.replace(/,/g, '')),
        };
        writeFileSync("aggregation.json", JSON.stringify(statementproof));
    }
    },
    options: { domainId: 0 },
  },
]);

  const { events } = await session.verify()
    .groth16({ library: Library.snarkjs, curve: CurveType.bls12381 })
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

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});