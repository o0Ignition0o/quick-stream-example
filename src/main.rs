// cargo run -- -s ./supergraph.graphql -c ./router.yaml

// Invalid Query: (the hello field doesnt exist)
// curl -v \
// --header 'apollographql-client-name: ignition' \
// --header 'apollographql-client-version: test' \
// --header 'content-type: application/json' \
// --url 'http://127.0.0.1:4000/' \
// --data '{"query":"query Me {\n  me {\n    name\n }\n}","variables":{}}'
// HTTP 200

// curl -v --request POST \
// --header 'content-type: application/json' \
// --url http://localhost:4000 \
// --data '{"query":"query TopProducts($first: Int) {\n  topProducts(first: $first) {\n    name\n  }\n}","variables":{"first":"coucou"}}'
// HTTP 401

mod plugins;

use anyhow::Result;

fn main() -> Result<()> {
    apollo_router::main()
}
