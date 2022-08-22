use apollo_router::plugin::{Plugin, PluginInit};
use apollo_router::register_plugin;
use apollo_router::stages;
use futures::StreamExt;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json_bytes::Value;
use tower::{BoxError, ServiceBuilder, ServiceExt as _};

#[derive(Debug)]
struct CatchInvalidTypeError {
    #[allow(dead_code)]
    configuration: Conf,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct Conf {
    enabled: bool,
}
#[async_trait::async_trait]
impl Plugin for CatchInvalidTypeError {
    type Config = Conf;

    async fn new(init: PluginInit<Self::Config>) -> Result<Self, BoxError> {
        tracing::info!("{}", init.config.enabled);
        Ok(CatchInvalidTypeError {
            configuration: init.config,
        })
    }

    fn supergraph_service(
        &self,
        service: stages::supergraph::BoxService,
    ) -> stages::supergraph::BoxService {
        ServiceBuilder::new()
            .service(service)
            .map_response(|supergraph_response| {
                // we will need to wait for the first response to check if there are errors
                supergraph_response.map(|response| {
                    Box::pin(response.map(move |body| {
                        // we have a router_response, let's see if we have an invalidtype error
                        let has_invalid_type_error = body.errors.iter().any(|e| {
                            e.extensions.get("type")
                                == Some(&Value::String("ValidationInvalidTypeVariable".into()))
                        });

                        if has_invalid_type_error {
                            tracing::info!("we have an invalid type error!");
                        } else {
                            tracing::info!("we don't an invalid type error!");
                        }
                        body
                    }))
                })
            })
            .boxed()
    }
}

// This macro allows us to use it in our plugin registry!
// register_plugin takes a group name, and a plugin name.
register_plugin!(
    "my_example",
    "catch_invalid_type_error",
    CatchInvalidTypeError
);

#[cfg(test)]
mod tests {
    use ::http::StatusCode;
    use apollo_router::stages::*;
    use tower::{BoxError, Service};

    #[tokio::test]
    async fn plugin_registered() {
        let config = serde_json::json!({
            "plugins": {
                "my_example.catch_invalid_type_error": {
                    "enabled" : true
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
    async fn basic_test() -> Result<(), BoxError> {
        // Define a configuration to use with our plugin
        let config = serde_json::json!({
            "plugins": {
                "my_example.catch_invalid_type_error": {
                    "enabled" : true
                }
            }
        });

        // Build an router with our plugin
        let mut test_harness = apollo_router::TestHarness::builder()
            .configuration_json(config)
            .unwrap()
            .build()
            .await
            .unwrap();

        // Send a request
        let valid_request = supergraph::Request::fake_builder()
            .query("query Me {\n  me {\n    name\n  }\n}")
            .build()?;
        let result = test_harness.call(valid_request).await?;
        assert_eq!(StatusCode::OK, result.response.status());

        let invalid_request = supergraph::Request::fake_builder()
            .query("query Me {\n  me {\n    name\n thisfielddoesntexist\n }\n}")
            .build()?;
        let result = test_harness.call(invalid_request).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, result.response.status());

        Ok(())
    }
}
