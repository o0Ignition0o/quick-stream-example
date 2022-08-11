use apollo_router::graphql::Response;
use apollo_router::layers::ServiceBuilderExt;
use apollo_router::plugin::{Plugin, PluginInit};
use apollo_router::{
    register_plugin,
    services::{
        ExecutionRequest, ExecutionResponse, RouterRequest, RouterResponse, SubgraphRequest,
        SubgraphResponse,
    },
    Context,
};
use futures::StreamExt;
use http::header::HeaderName;
use http::HeaderValue;
use schemars::JsonSchema;
use serde::Deserialize;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tower::util::BoxService;
use tower::{BoxError, ServiceBuilder, ServiceExt};

#[derive(Debug)]
struct ResponseStreamContextPropagation {
    #[allow(dead_code)]
    configuration: Conf,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct Conf {
    enabled: bool,
}
#[async_trait::async_trait]
impl Plugin for ResponseStreamContextPropagation {
    type Config = Conf;

    async fn new(init: PluginInit<Self::Config>) -> Result<Self, BoxError> {
        tracing::info!("{}", init.config.enabled);
        Ok(ResponseStreamContextPropagation {
            configuration: init.config,
        })
    }

    // Delete this function if you are not customizing it.
    fn router_service(
        &self,
        service: BoxService<RouterRequest, RouterResponse, BoxError>,
    ) -> BoxService<RouterRequest, RouterResponse, BoxError> {
        ServiceBuilder::new()
            .service(service)
            .map_response(|mut router_response| {
                if let Ok(Some(true)) = router_response.context.get::<_, bool>("debug") {
                    tracing::info!("debug mode!");

                    // after-first-response-context has *not* been set yet
                    assert!(router_response
                        .context
                        .get::<_, u8>("after-first-response-context")
                        .unwrap()
                        .is_none());

                    // let's get subgraph response times from the context!
                    let subgraph_response_times = router_response
                        .context
                        .get::<_, Vec<(String, Duration)>>("subgraph-response-times")
                        .expect("couldn't get a value from the context");

                    if let Some(response_times) = subgraph_response_times {
                        for (subgraph_name, duration) in response_times.iter() {
                            tracing::info!("subgraph {} replied in {:?}", subgraph_name, duration);
                            // let's add the subgraph response time in the header:
                            router_response.response.headers_mut().append(
                                HeaderName::from_str(
                                    format!("subgraph-response-time-{}", subgraph_name).as_str(),
                                )
                                .expect("couldn't create header name"),
                                HeaderValue::from_str(format!("{:?}", duration).as_str())
                                    .expect("couldn't create header value"),
                            );
                        }
                    } else {
                        // this part is useful for tests, where subgraphs won't run so the key won't be present
                        tracing::info!("no subgraph response times!");
                    }

                    // we need to clone the context in order to use it in the closure
                    let context = router_response.context.clone();

                    // we can now transform the router_response!
                    router_response.map(|response| {
                        // we need to clone the context in order to use it in the closure
                        let context = context.clone();
                        Box::pin(response.map(move |body| handle_router_response(body, &context)))
                    })
                } else {
                    router_response
                }
            })
            .boxed()
    }

    // Delete this function if you are not customizing it.
    fn execution_service(
        &self,
        service: BoxService<ExecutionRequest, ExecutionResponse, BoxError>,
    ) -> BoxService<ExecutionRequest, ExecutionResponse, BoxError> {
        ServiceBuilder::new()
            .service(service)
            .map_response(|execution_response| {
                execution_response.context.insert("debug", true).unwrap();
                // we need to clone the context in order to use it in the closure
                let context = execution_response.context.clone();
                execution_response.map(|response| {
                    // we need to clone the context in order to use it in the closure
                    let context = context.clone();
                    Box::pin(response.map(move |body| handle_execution_response(body, &context)))
                })
            })
            .boxed()
    }

    // Delete this function if you are not customizing it.
    fn subgraph_service(
        &self,
        service_name: &str,
        service: BoxService<SubgraphRequest, SubgraphResponse, BoxError>,
    ) -> BoxService<SubgraphRequest, SubgraphResponse, BoxError> {
        // let's keep the service_name around, so we can use it in the map_future_with_context closure
        let service_name = service_name.to_string();
        ServiceBuilder::new()
            // we're going to use map_future_with_context here so we can start a timer,
            // and insert the elapsed duration in the context once the subgraph call is done
            .map_future_with_context(
                move |req: &SubgraphRequest| req.context.clone(),
                move |ctx: Context, fut| {
                    // start a timer
                    let start = Instant::now();

                    // we're cloning service name so we can use it in the async closure
                    let service_name = service_name.clone();
                    async move {
                        // run the subgraph request
                        let result: Result<SubgraphResponse, BoxError> = fut.await;
                        // get the duration
                        let duration = start.elapsed();
                        // add this timer to subgraph-response-times.
                        // we want to push the subgraph response time to an array
                        // we will use context.upsert to do that
                        ctx.upsert(
                            "subgraph-response-times",
                            |mut response_times: Vec<(String, Duration)>| {
                                response_times.push((service_name.clone(), duration));
                                tracing::info!("pushed response time!");
                                response_times
                            },
                        )
                        .expect("couldn't put subgraph response time to the context");
                        // finally we can return the future result
                        result
                    }
                },
            )
            .service(service)
            .boxed()
    }
}

fn handle_router_response(body: Response, context: &Context) -> Response {
    // after-first-response-context is available here!
    assert_eq!(
        42,
        context
            .get::<_, u8>("after-first-response-context")
            .unwrap()
            .unwrap()
    );

    tracing::info!("got non primary router response body! {:?}", body);
    body
}

fn handle_execution_response(body: Response, context: &Context) -> Response {
    // let's add information through the context
    context.insert("after-first-response-context", 42).unwrap();
    tracing::info!("got non primary execution response body! {:?}", body);
    body
}

// This macro allows us to use it in our plugin registry!
// register_plugin takes a group name, and a plugin name.
register_plugin!(
    "my_example",
    "response_stream_context_propagation",
    ResponseStreamContextPropagation
);

#[cfg(test)]
mod tests {
    use super::{Conf, ResponseStreamContextPropagation};
    use apollo_router::plugin::test::IntoSchema::Canned;
    use apollo_router::plugin::{plugins, test::PluginTestHarness, Plugin, PluginInit};
    use std::sync::Arc;
    use tower::BoxError;

    #[tokio::test]
    async fn plugin_registered() {
        plugins()
            .get("my_example.response_stream_context_propagation")
            .expect("Plugin not found")
            .create_instance(
                &serde_json::json!({"enabled" : true}),
                Arc::new("".to_string()),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn basic_test() -> Result<(), BoxError> {
        // Define a configuration to use with our plugin
        let conf = Conf { enabled: true };

        // Build an instance of our plugin to use in the test harness
        let plugin =
            ResponseStreamContextPropagation::new(PluginInit::new(conf, Arc::new("".to_string())))
                .await
                .expect("created plugin");

        // Create the test harness. You can add mocks for individual services, or use prebuilt canned services.
        let mut test_harness = PluginTestHarness::builder()
            .plugin(plugin)
            .schema(Canned)
            .build()
            .await?;

        // Send a request
        let mut result = test_harness.call_canned().await?;

        let first_response = result
            .next_response()
            .await
            .expect("couldn't get primary response");

        assert!(first_response.data.is_some());

        // You could keep calling result.next_response() until it yields None if you're expexting more parts.
        assert!(result.next_response().await.is_none());
        Ok(())
    }
}
