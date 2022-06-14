fn router_service(service) {
    let f = |response| {
        print("router complete!");
    };
    service.map_response(f);

    print("[rhai] router service is set up!");
}

fn query_planner_service(service) {
    let f = |response| {
        print("query planning complete!");
    };
    service.map_response(f);

    print("[rhai] query_planner service is set up!");
}

fn execution_service(service) {
    let f = |response| {
        print("execution complete!");
    };
    service.map_response(f);

    print("[rhai] execution service is set up!");
}

fn subgraph_service(service, subgraph) {
    print("hello from subgraph service");
    let f = |response| {
        print("subgraph complete!");
    };
    service.map_response(f);

    print("[rhai] subgraph service is set up!");
}