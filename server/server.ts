import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import path from "path";
import { Service } from "./services/service";
import { VoteResult } from "./interface/gov";

const SERVICE_PROTO_PATH = path.join(__dirname, "../proto/service.proto");
const QUERY_PROTO_PATH = path.join(__dirname, "../proto/query.proto");

// Load proto definition
const servicePackageDef = protoLoader.loadSync(SERVICE_PROTO_PATH, {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});

const queryPackageDef = protoLoader.loadSync(QUERY_PROTO_PATH, {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});

const servicesProto = grpc.loadPackageDefinition(servicePackageDef).gov as any;
const queryProto = grpc.loadPackageDefinition(queryPackageDef).gov as any;

// Start server
function main() {
  const server = new grpc.Server();
  let serviceHandler = new Service();
  server.addService(servicesProto.Governance.service, {
    SubmitVote: serviceHandler.submitVote.bind(serviceHandler),
    SubmitProposal: serviceHandler.submitProposal.bind(serviceHandler),
    CreateGroup: serviceHandler.createGroup.bind(serviceHandler),
  });
  server.addService(queryProto.Query.service, {
    Commitments: serviceHandler.commitments.bind(serviceHandler),
    VoteResult: serviceHandler.VoteResult.bind(serviceHandler)
  });

  const addr = "0.0.0.0:50051";
  server.bindAsync(addr, grpc.ServerCredentials.createInsecure(), (err, port) => {
    if (err) {
      console.error(err);
      return;
    }
    console.log(`🚀 gRPC server running at ${addr}`);
  });
}

main();
