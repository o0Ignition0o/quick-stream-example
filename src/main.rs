// cargo run -- -s ./supergraph.graphql -c ./router.yaml

// curl -v \
// --header 'content-type: application/json' \
// --url 'http://127.0.0.1:4000/' \
// --data '{"query":"query Me {\n  me {\n    name\n  }\n}","variables":{}}'
//
//
// *   Trying 127.0.0.1:4000...
// * Connected to 127.0.0.1 (127.0.0.1) port 4000 (#0)
// > POST / HTTP/1.1
// > Host: 127.0.0.1:4000
// > User-Agent: curl/7.79.1
// > Accept: */*
// > content-type: application/json
// > Content-Length: 63
// >
// * Mark bundle as not supporting multiuse
// < HTTP/1.1 200 OK
// < subgraph-response-time-accounts: 151.496791ms
// < content-type: application/json
// < content-length: 39
// < vary: origin
// < vary: access-control-request-method
// < vary: access-control-request-headers
// < date: Wed, 15 Jun 2022 13:28:02 GMT
// <
// * Connection #0 to host 127.0.0.1 left intact
// {"data":{"me":{"name":"Ada Lovelace"}}}

mod plugins;

use anyhow::Result;

fn main() -> Result<()> {
    apollo_router::main()
}
