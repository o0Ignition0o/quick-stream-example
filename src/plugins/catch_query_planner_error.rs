// use apollo_router::error::SpecError;
use apollo_router::layers::ServiceExt;
use apollo_router::plugin::{Plugin, PluginInit};
use apollo_router::stages::*;
use apollo_router::{register_plugin, Context};
use http::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use tower::{BoxError, ServiceBuilder, ServiceExt as _};

#[derive(Debug)]
struct CatchQueryPlannerError {
    #[allow(dead_code)]
    configuration: Conf,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct Conf {
    enabled: bool,
}
#[async_trait::async_trait]
impl Plugin for CatchQueryPlannerError {
    type Config = Conf;

    async fn new(init: PluginInit<Self::Config>) -> Result<Self, BoxError> {
        tracing::info!("{}", init.config.enabled);
        Ok(CatchQueryPlannerError {
            configuration: init.config,
        })
    }

    fn router_service(&self, service: router::BoxService) -> router::BoxService {
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
        service: query_planner::BoxService,
    ) -> query_planner::BoxService {
        ServiceBuilder::new()
            .service(service)
            .map_future_with_context(
                move |req: &query_planner::Request| req.context.clone(),
                |_ctx: Context, query_planner_future| async move {
                    // let's run the query planner
                    let query_planner_response: Result<query_planner::Response, BoxError> =
                        query_planner_future.await;

                    // TODO: wait until the query planner refacto lands, and update the example

                    // if let Err(error) = &query_planner_response {
                    //     // Ok this one is a bit tricky, but bear with me:
                    //     //
                    //     // The error here is a BoxError, we will try to downcast it into the error we are looking for...
                    //     if let Some(CacheResolverError::RetrievalError(error)) =
                    //         error.downcast_ref()
                    //     {
                    //         if let QueryPlannerError::SpecError(SpecError::InvalidType(message)) =
                    //             error.as_ref()
                    //         {
                    //             // We could even check if the message is about a specific type,
                    //             // but the example is big enough already.
                    //             tracing::info!("got an invalid type error: {message}");
                    //             // let's use the context to declare our intention to turn the graphql response into a 401
                    //             // TODO: handle failed insert?
                    //             let _ = ctx.insert("set_status_code", 401u16);
                    //         }
                    //     }
                    // }
                    query_planner_response
                },
            )
            .boxed()
    }
}

// This macro allows us to use it in our plugin registry!
// register_plugin takes a group name, and a plugin name.
register_plugin!(
    "my_example",
    "catch_query_planner_error",
    CatchQueryPlannerError
);

#[cfg(test)]
mod tests {
    use apollo_router::stages::router;
    use http::StatusCode;
    use tower::Service;

    #[tokio::test]
    async fn plugin_registered() {
        let config = serde_json::json!({
            "plugins": {
                "my_example.catch_query_planner_error": {
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
                "my_example.catch_query_planner_error": {
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

        // Send a valid request
        let valid_request = router::Request::fake_builder()
            .query("query Me {\n  me {\n    name\n  }\n}")
            .build()
            .expect("couldn't craft request");

        let mut result = test_harness
            .call(valid_request)
            .await
            .expect("service call failed");

        assert_eq!(StatusCode::OK, result.response.status());

        assert!(result.next_response().await.is_some());
        // You could keep calling result.next_response() until it yields None if you're expexting more parts.
        assert!(result.next_response().await.is_none());

        // Send an invalid request
        let invalid_request = router::Request::fake_builder()
            .query("query Me {\n  me {\n    name\n thisfielddoesntexist\n }\n}")
            .build()
            .expect("couldn't craft request");

        let _result = test_harness
            .call(invalid_request)
            .await
            .expect("service call failed");

        // TODO: wait until the query planner refacto lands, and make the test pass
        // assert_eq!(StatusCode::UNAUTHORIZED, result.response.status());
    }
}
