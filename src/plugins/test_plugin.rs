use apollo_router::graphql::Response;
use apollo_router::plugin::Plugin;
use apollo_router::register_plugin;
use apollo_router::services::{RouterRequest, RouterResponse, SubgraphRequest, SubgraphResponse};
use futures::stream::BoxStream;
use http::StatusCode;
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

    fn router_service(
        &self,
        service: BoxService<RouterRequest, RouterResponse<BoxStream<'static, Response>>, BoxError>,
    ) -> BoxService<RouterRequest, RouterResponse<BoxStream<'static, Response>>, BoxError> {
        ServiceBuilder::new()
            .service(service)
            .map_response(|mut router_response| {
                // let's see if a subgraph service has set a response status
                if let Ok(Some(status_code_to_set)) = router_response.context.get("set_status_code")
                {
                    // let's set the response status
                    let response_status = router_response.response.status_mut();
                    *response_status =
                        StatusCode::from_u16(status_code_to_set).unwrap_or_else(|_| StatusCode::OK);
                }
                router_response
            })
            .boxed()
    }

    fn subgraph_service(
        &self,
        _service_name: &str,
        service: BoxService<SubgraphRequest, SubgraphResponse, BoxError>,
    ) -> BoxService<SubgraphRequest, SubgraphResponse, BoxError> {
        ServiceBuilder::new()
            .service(service)
            // we're going to use map_future_with_context here so we can start a timer,
            // and insert the elapsed duration in the context once the subgraph call is done
            .map_response(|res: SubgraphResponse| {
                // we have a subgraphresponse here, we could have a look at the status code for example:

                if res.response.status() == 200 {
                    // Sneaky attempt to turn http 200 into http 401
                    // we do this by using the context
                    // to show our intent to change the status code,
                    // which the router service will pick up later on
                    let _ = res.context.insert("set_status_code", 401u16); // TODO: handle insertion error maybe?
                }

                res
            })
            .boxed()
    }
}

// This macro allows us to use it in our plugin registry!
// register_plugin takes a group name, and a plugin name.
register_plugin!("my_example", "test_plugin", TestPlugin);

#[cfg(test)]
mod tests {
    use super::{Conf, TestPlugin};
    use apollo_router::plugin::test::IntoSchema::Canned;
    use apollo_router::plugin::{plugins, test::PluginTestHarness, Plugin};
    use http::StatusCode;
    use tower::BoxError;

    #[tokio::test]
    async fn plugin_registered() {
        plugins()
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

        assert_eq!(StatusCode::UNAUTHORIZED, result.response.status());

        assert!(result.next_response().await.is_some());

        // You could keep calling result.next_response() until it yields None if you're expexting more parts.
        assert!(result.next_response().await.is_none());
        Ok(())
    }
}
