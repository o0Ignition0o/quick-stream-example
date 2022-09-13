use std::pin::Pin;

use apollo_router::layers::ServiceExt;
use apollo_router::plugin::{Plugin, PluginInit};
use apollo_router::register_plugin;
use apollo_router::services;
use futures::StreamExt;
use http::StatusCode;
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
        service: services::supergraph::BoxService,
    ) -> services::supergraph::BoxService {
        if self.configuration.enabled {
            service
        } else {
            ServiceBuilder::new()
                .service(service)
                .map_future(|fut| async {
                    let response: services::supergraph::Response = fut.await?;
                    let services::supergraph::Response {
                        response: response_body,
                        context,
                        ..
                    } = response;
                    let (mut parts, body) = response_body.into_parts();
                    let mut peekable = body.peekable();
                    let pinned = Pin::new(&mut peekable);
                    if let Some(first_payload) = pinned.peek().await {
                        let has_invalid_type_error = first_payload.errors.iter().any(|e| {
                            e.extensions.get("type")
                                == Some(&Value::String("ValidationInvalidTypeVariable".into()))
                        });
                        if has_invalid_type_error {
                            tracing::info!("we have an invalid type error!");
                            parts.status = StatusCode::UNAUTHORIZED;
                        } else {
                            tracing::info!("we don't have an invalid type error!");
                        }
                    }
                    Ok(services::supergraph::Response {
                        response: http::Response::from_parts(parts, peekable.boxed()).into(),
                        context,
                    })
                })
                .map_first_graphql_response(|_, mut parts, response| {
                    let has_invalid_type_error = dbg!(&response.errors).iter().any(|e| {
                        e.extensions.get("type")
                            == Some(&Value::String("ValidationInvalidTypeVariable".into()))
                    });
                    if has_invalid_type_error {
                        tracing::info!("we have an invalid type error!");
                        parts.status = StatusCode::UNAUTHORIZED;
                    } else {
                        tracing::info!("we don't have an invalid type error!");
                    }

                    (parts, response)
                })
                .boxed()
        }
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
    use apollo_router::services::*;
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
            .schema(include_str!("../../supergraph.graphql"))
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
            .query("query TopProducts($first: Int) {\n  topProducts(first: $first) {\n    name\n  }\n}")
            .variable("first", "thisShouldBeAnInteger")
            .build()?;
        let result = test_harness.call(invalid_request).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, result.response.status());

        Ok(())
    }
}
