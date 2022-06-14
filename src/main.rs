// cargo run -- -s ./supergraph.graphql -c ./router.yaml

// curl --request POST \
// --header 'apollographql-client-name: ignition' \
// --header 'apollographql-client-version: test' \
// --header 'content-type: application/json' \
// --url 'http://127.0.0.1:4000/' \
// --data '{"query":"query Me {\n  me {\n    name\n  }\n}","variables":{}}'

mod plugins;

use anyhow::Result;

fn main() -> Result<()> {
    apollo_router::main()
}
