use apollo_router::plugin::{Plugin, PluginInit};
use apollo_router::register_plugin;
use apollo_router::stages::*;
use http::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use tower::{BoxError, ServiceBuilder, ServiceExt};

#[derive(Debug)]
struct ChangeStatusCode {
    #[allow(dead_code)]
    configuration: Conf,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct Conf {
    enabled: bool,
}
#[async_trait::async_trait]
impl Plugin for ChangeStatusCode {
    type Config = Conf;

    async fn new(init: PluginInit<Self::Config>) -> Result<Self, BoxError> {
        tracing::info!("{}", init.config.enabled);
        Ok(ChangeStatusCode {
            configuration: init.config,
        })
    }

    fn router_service(&self, service: router::BoxService) -> router::BoxService {
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
        service: subgraph::BoxService,
    ) -> subgraph::BoxService {
        ServiceBuilder::new()
            .service(service)
            // we're going to use map_future_with_context here so we can start a timer,
            // and insert the elapsed duration in the context once the subgraph call is done
            .map_response(|res: subgraph::Response| {
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
register_plugin!("my_example", "change_status_code", ChangeStatusCode);

#[cfg(test)]
mod tests {
    use apollo_router::stages::router;
    use http::StatusCode;
    use tower::Service;

    #[tokio::test]
    async fn plugin_registered() {
        let config = serde_json::json!({
            "plugins": {
                "my_example.change_status_code": {
                    "enabled": true ,
                }
            }
        });

        apollo_router::TestHarness::builder()
            .configuration_json(config)
            .unwrap()
            .build()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn basic_test() {
        // Define a configuration to use with our plugin
        let config = serde_json::json!({
            "plugins": {
                "my_example.change_status_code": {
                    "enabled": true ,
                }
            }
        });

        // Spin up a test harness with the plugin enabled
        let mut test_harness = apollo_router::TestHarness::builder()
            .configuration_json(config)
            .unwrap()
            .build()
            .await
            .unwrap();

        // Send a request
        let request = router::Request::fake_builder()
            .build()
            .expect("couldn't craft request");

        let mut result = test_harness
            .call(request)
            .await
            .expect("service call failed");

        assert_eq!(StatusCode::UNAUTHORIZED, result.response.status());

        assert!(result.next_response().await.is_some());

        // You could keep calling result.next_response() until it yields None if you're expexting more parts.
        assert!(result.next_response().await.is_none());
    }
}
