use apollo_router::error::{CacheResolverError, QueryPlannerError, SpecError};
use apollo_router::layers::ServiceExt;
use apollo_router::plugin::{Plugin, PluginInit};
use apollo_router::services::{
    QueryPlannerRequest, QueryPlannerResponse, RouterRequest, RouterResponse,
};
use apollo_router::{register_plugin, Context};
use http::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use tower::util::BoxService;
use tower::{BoxError, ServiceBuilder, ServiceExt as _};

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

    async fn new(init: PluginInit<Self::Config>) -> Result<Self, BoxError> {
        tracing::info!("{}", init.config.enabled);
        Ok(TestPlugin {
            configuration: init.config,
        })
    }

    fn router_service(
        &self,
        service: BoxService<RouterRequest, RouterResponse, BoxError>,
    ) -> BoxService<RouterRequest, RouterResponse, BoxError> {
        ServiceBuilder::new()
            .service(service)
            .map_response(|mut router_response| {
                // let's see if we need to set a custom response status
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

    /// This service handles generating the query plan for each incoming request.
    /// Define `query_planning_service` if your customization needs to interact with query planning functionality (for example, to log query plan details).
    fn query_planning_service(
        &self,
        service: BoxService<QueryPlannerRequest, QueryPlannerResponse, BoxError>,
    ) -> BoxService<QueryPlannerRequest, QueryPlannerResponse, BoxError> {
        ServiceBuilder::new()
            .service(service)
            .map_future_with_context(
                move |req: &QueryPlannerRequest| req.context.clone(),
                |ctx: Context, query_planner_future| async move {
                    // let's run the query planner
                    let query_planner_response: Result<QueryPlannerResponse, BoxError> =
                        query_planner_future.await;

                    match &query_planner_response {
                        Err(error) => {
                            // Ok this one is a bit tricky, but bear with me:
                            //
                            // The error here is a BoxError, we will try to downcast it into the error we are looking for...
                            match error.downcast_ref() {
                                Some(CacheResolverError::RetrievalError(error)) => {
                                    match error.as_ref() {
                                        // We're dealing with an invalid type error
                                        QueryPlannerError::SpecError(SpecError::InvalidType(
                                            message,
                                        )) => {
                                            // We could even check if the message is about a specific type,
                                            // but the example is big enough already.
                                            tracing::info!("got an invalid type error: {message}");
                                            // let's use the context to declare our intention to turn the graphql response into a 401
                                            // TODO: handle failed insert?
                                            let _ = ctx.insert("set_status_code", 401u16);
                                        }
                                        _ => {
                                            // this is not the error we are looking for
                                        }
                                    }
                                }
                                _ => {
                                    // This is not a query planner error
                                }
                            }
                        }
                        // the success variant isn't interesting to us.
                        _ => {}
                    };
                    query_planner_response
                },
            )
            .boxed()
    }
}

// This macro allows us to use it in our plugin registry!
// register_plugin takes a group name, and a plugin name.
register_plugin!("my_example", "test_plugin", TestPlugin);

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{Conf, TestPlugin};
    use apollo_router::plugin::test::IntoSchema::Canned;
    use apollo_router::plugin::PluginInit;
    use apollo_router::plugin::{plugins, test::PluginTestHarness, Plugin};
    use apollo_router::services::RouterRequest;
    use http::StatusCode;
    use tower::BoxError;

    #[tokio::test]
    async fn plugin_registered() {
        plugins()
            .get("my_example.test_plugin")
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
        let plugin = TestPlugin::new(PluginInit::new(conf, Arc::new("".to_string())))
            .await
            .expect("created plugin");

        // Create the test harness. You can add mocks for individual services, or use prebuilt canned services.
        let mut test_harness = PluginTestHarness::builder()
            .plugin(plugin)
            .schema(Canned)
            .build()
            .await?;

        // Send a valid request
        let valid_request = RouterRequest::fake_builder()
            .query("query Me {\n  me {\n    name\n  }\n}")
            .build()?;
        let mut result = test_harness.call(valid_request).await?;

        assert_eq!(StatusCode::OK, result.response.status());

        assert!(result.next_response().await.is_some());
        // You could keep calling result.next_response() until it yields None if you're expexting more parts.
        assert!(result.next_response().await.is_none());

        // Send an invalid request
        let invalid_request = RouterRequest::fake_builder()
            .query("query Me {\n  me {\n    name\n thisfielddoesntexist\n }\n}")
            .build()?;
        let result = test_harness.call(invalid_request).await?;

        assert_eq!(StatusCode::UNAUTHORIZED, result.response.status());
        Ok(())
    }
}
