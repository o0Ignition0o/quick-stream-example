use std::time::{Duration, Instant};

use apollo_router::plugin::Plugin;
use apollo_router::{
    register_plugin, Context, ExecutionRequest, ExecutionResponse, Response, ResponseBody,
    RouterRequest, RouterResponse, ServiceBuilderExt, SubgraphRequest, SubgraphResponse,
};
use futures::stream::BoxStream;
use futures::StreamExt;
use schemars::JsonSchema;
use serde::Deserialize;
use tower::util::BoxService;
use tower::{BoxError, ServiceBuilder, ServiceExt};

#[derive(Debug)]
struct TestPlugin {
    #[allow(dead_code)]
    configuration: Conf,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct Conf {
    enabled: bool,
}
#[async_trait::async_trait]
impl Plugin for TestPlugin {
    type Config = Conf;

    async fn new(configuration: Self::Config) -> Result<Self, BoxError> {
        tracing::info!("{}", configuration.enabled);
        Ok(TestPlugin { configuration })
    }

    // Delete this function if you are not customizing it.
    fn router_service(
        &mut self,
        service: BoxService<
            RouterRequest,
            RouterResponse<BoxStream<'static, ResponseBody>>,
            BoxError,
        >,
    ) -> BoxService<RouterRequest, RouterResponse<BoxStream<'static, ResponseBody>>, BoxError> {
        ServiceBuilder::new()
            .service(service)
            .map_response(|router_response| {
                if let Ok(Some(true)) = router_response.context.get::<_, bool>("debug") {
                    tracing::info!("debug mode!");

                    // after-response-value has *not* been set yet
                    assert!(router_response
                        .context
                        .get::<_, u8>("after-response-value")
                        .unwrap()
                        .is_none());

                    // let's play with the headers!
                    let headers = router_response
                        .response
                        .headers()
                        .iter()
                        .map(|(key, value)| format!("{}: {:?}", key.to_string(), value))
                        .collect::<Vec<_>>()
                        .join(" ");
                    tracing::info!("headers are: {:?}", headers);

                    // we need to clone the context in order to use it in the closure
                    let context = router_response.context.clone();

                    // we can now transform the router_response!
                    router_response
                        .map(|response| {
                            // we need to clone the context in order to use it in the closure
                            let context = context.clone();
                            response.map(move |body| handle_router_response(body, &context))
                        })
                        .boxed()
                } else {
                    router_response.boxed()
                }
            })
            .boxed()
    }

    // Delete this function if you are not customizing it.
    fn execution_service(
        &mut self,
        service: BoxService<
            ExecutionRequest,
            ExecutionResponse<BoxStream<'static, Response>>,
            BoxError,
        >,
    ) -> BoxService<ExecutionRequest, ExecutionResponse<BoxStream<'static, Response>>, BoxError>
    {
        ServiceBuilder::new()
            .service(service)
            .map_response(|execution_response| {
                execution_response.context.insert("debug", true).unwrap();
                // we need to clone the context in order to use it in the closure
                let context = execution_response.context.clone();
                execution_response
                    .map(|response| {
                        // we need to clone the context in order to use it in the closure
                        let context = context.clone();
                        response.map(move |body| handle_execution_response(body, &context))
                    })
                    .boxed()
            })
            .boxed()
    }

    // Delete this function if you are not customizing it.
    fn subgraph_service(
        &mut self,
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

fn handle_router_response(body: ResponseBody, context: &Context) -> ResponseBody {
    // after-response-value is available here!
    assert_eq!(
        42,
        context
            .get::<_, u8>("after-response-value")
            .unwrap()
            .unwrap()
    );

    // now let's get subgraph response times from the context!
    let subgraph_response_times = context
        .get::<_, Vec<(String, Duration)>>("subgraph-response-times")
        .expect("couldn't get a value from the context");

    if let Some(response_times) = subgraph_response_times {
        for (subgraph_name, duration) in response_times.iter() {
            tracing::info!("subgraph {} replied in {:?}", subgraph_name, duration);
        }
    } else {
        // this part is useful for tests, where subgraphs won't run so the key won't be present
        tracing::warn!("no subgraph response times!");
    }

    tracing::info!("got router response body! {:?}", body);
    body
}

fn handle_execution_response(body: Response, context: &Context) -> Response {
    // let's add information through the context
    context.insert("after-response-value", 42).unwrap();
    tracing::info!("got execution response body! {:?}", body);
    body
}

// This macro allows us to use it in our plugin registry!
// register_plugin takes a group name, and a plugin name.
register_plugin!("my_example", "test_plugin", TestPlugin);

#[cfg(test)]
mod tests {
    use super::{Conf, TestPlugin};

    use apollo_router::utils::test::IntoSchema::Canned;
    use apollo_router::utils::test::PluginTestHarness;
    use apollo_router::{Plugin, ResponseBody};
    use tower::BoxError;

    #[tokio::test]
    async fn plugin_registered() {
        apollo_router::plugins()
            .get("my_example.test_plugin")
            .expect("Plugin not found")
            .create_instance(&serde_json::json!({"enabled" : true}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn basic_test() -> Result<(), BoxError> {
        // Define a configuration to use with our plugin
        let conf = Conf { enabled: true };

        // Build an instance of our plugin to use in the test harness
        let plugin = TestPlugin::new(conf).await.expect("created plugin");

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

        if let ResponseBody::GraphQL(graphql) = first_response {
            assert!(graphql.data.is_some());
        } else {
            panic!("expected graphql response")
        }

        // You could keep calling result.next_response() until it yields None if you're expexting more parts.
        assert!(result.next_response().await.is_none());
        Ok(())
    }
}
