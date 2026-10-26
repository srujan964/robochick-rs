use std::collections::HashMap;

use anyhow::anyhow;
use aws_config::{BehaviorVersion, meta::region::RegionProviderChain};
use aws_sdk_dynamodb::Client;
use axum::{
    Router,
    extract::{Query, Request, State},
    http::HeaderMap,
    routing::{get, post},
};
use lambda_http::{
    Body, Error, Response,
    tracing::{self, instrument, subscriber},
};
use reqwest::{StatusCode, Url};
use tokio::sync::OnceCell;

use robochick_rs::{
    auth,
    client::WebClient,
    config::AppConfig,
    handler::event_handler::EventHandler,
    reward::{ducks::DuckRedeemed, mod_feeder::ModFeed},
    robochick::twitch::MessageBuilder,
};

#[derive(Clone)]
struct AppState {
    config: AppConfig,
    dynamo_client: Client,
}

impl AppState {
    fn new(config: AppConfig, dynamo_client: Client) -> Self {
        AppState {
            config,
            dynamo_client,
        }
    }
}

async fn healthcheck() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("bokbokbok"))
        .unwrap()
}

async fn oauth_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response<Body> {
    if let (Some(code), Some(scope)) = (params.get("code"), params.get("scope")) {
        let url_base = format!("{}/oauth2/token", state.config.twitch_host);
        let req_params = [
            ("client_id", state.config.twitch_client_id),
            ("client_secret", state.config.twitch_client_secret.unwrap()),
            ("code", code.to_string()),
            ("grant_type", "authorization_code".to_string()),
            ("redirect_uri", state.config.redirect_uri),
        ];
        let url = Url::parse_with_params(&url_base, req_params.iter());

        let resp = match reqwest::Client::new().post(url.unwrap()).send().await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!(
                    original_err = %e.without_url(),
                    "Failed to create auth token via /oauth2/token"
                );
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::Empty)
                    .unwrap();
            }
        };

        let oauth_response = match resp.text().await {
            Ok(response) => response,
            Err(e) => {
                tracing::error!("Error decoding data from oauth API response");
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::Empty)
                    .unwrap();
            }
        };

        match auth::securely_store_oauth_tokens(oauth_response, state.config.auth_secret_name).await
        {
            Ok(secret_name) => tracing::info!(
                secret = %secret_name,
                "Successfully stored oauth token"
            ),
            Err(e) => tracing::error!(err = %e,"Failed to store oauth response"),
        }

        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")
            .body(Body::from("Authorized! Have a nice day!"))
            .unwrap();
    }

    if let (Some(error), Some(error_desc)) = (params.get("error"), params.get("error_description"))
    {
        tracing::info!(
             %error, %error_desc ,
            "Authorization denied by user :("
        );

        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")
            .body(Body::from("Authorization denied :("))
            .unwrap();
    }

    tracing::error!("Authorization request from Twitch is missing code and/or scopes param");

    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::from("Malformed auth request."))
        .unwrap()
}

async fn eventsub_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let client = reqwest::Client::new();
    let webclient = WebClient::new(client);

    let mut event_handler = EventHandler::default();
    event_handler.register(
        &state.config.feed_mods_rewards_id,
        ModFeed { client: webclient },
    );
    event_handler.register(
        &state.config.rubberduck_rewards_id,
        DuckRedeemed {
            dynamo_client: state.dynamo_client,
        },
    );

    match event_handler.handle(body, &headers, &state.config).await {
        Ok(resp) => {
            tracing::info!("Successfully handled event");
            resp
        }
        Err(e) => {
            tracing::error!(err = %e, "Event handling failed");

            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::Empty)
                .unwrap()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_current_span(false)
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .init();

    let config = AppConfig::from_env();
    let aws_cfg = aws_config::from_env()
        .region(RegionProviderChain::default_provider().or_else("eu-west-2"))
        .load()
        .await;

    let dynamo_client = Client::new(&aws_cfg);
    let state = AppState::new(config, dynamo_client);

    let app = Router::new()
        .route("/health", get(healthcheck))
        .route("/twitch/oauth", get(oauth_handler))
        .route("/twitch/eventsub", post(eventsub_handler))
        .with_state(state.clone());

    #[cfg(debug_assertions)]
    {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.map_err(Error::from)
    }

    #[cfg(not(debug_assertions))]
    {
        lambda_http::run(app).await
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use dotenvy::dotenv;

    use robochick_rs::config::AppConfig;

    #[test]
    fn from_env_creates_config() -> Result<()> {
        dotenvy::from_filename(".env.test")?;
        let _result = AppConfig::from_env();
        Ok(())
    }
}
