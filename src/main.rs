use std::collections::HashMap;

use anyhow::anyhow;
use axum::{
    Router,
    extract::{Query, Request, State},
    http::HeaderMap,
    routing::{get, post},
};
use lambda_http::{Body, Error, Response};
use reqwest::{StatusCode, Url};

use crate::{client::WebClient, config::AppConfig, handler::event_handler::EventHandler};

mod auth;
mod client;
mod handler;
mod robochick;
mod types;

pub mod config {
    use std::env;

    #[derive(Clone, PartialEq, Debug)]
    pub struct AppConfig {
        pub twitch_client_id: String,
        pub twitch_client_secret: Option<String>,
        pub twitch_eventsub_subscription_secret: String,
        pub twitch_channel_id: String,
        pub twitch_host: String,
        pub se_jwt: Option<String>,
        pub se_api_host: String,
        pub feed_mods_rewards_id: String,
        pub broadcaster_user_id: String,
        pub redirect_uri: String,
        pub message_components_config_path: String,
    }

    impl AppConfig {
        pub fn from_env() -> AppConfig {
            AppConfig {
                twitch_client_id: env::var("TWITCH_CLIENT_ID")
                    .expect("Missing TWITCH_CLIENT_ID env var"),
                twitch_client_secret: env::var("TWITCH_CLIENT_SECRET").ok(),
                twitch_eventsub_subscription_secret: env::var(
                    "TWITCH_EVENTSUB_SUBSCRIPTION_SECRET",
                )
                .expect("Missing TWITCH_EVENTSUB_SUBSCRIPTION_SECRET env var"),
                twitch_channel_id: env::var("TWITCH_CHANNEL_ID")
                    .expect("Missing TWITCH_CHANNEL_ID env var"),
                twitch_host: env::var("TWITCH_HOST").expect("Missing TWITCH_HOST env var"),
                se_jwt: env::var("SE_JWT").ok(),
                se_api_host: env::var("SE_API_HOST").expect("Missing SE_API_HOST env var"),
                feed_mods_rewards_id: env::var("FEED_MODS_REWARD_ID")
                    .expect("Missing FEED_MODS_REWARD_ID env var"),
                broadcaster_user_id: env::var("BROADCASTER_USER_ID")
                    .expect("Missing BROADCASTER_USER_ID env var"),
                redirect_uri: env::var("REDIRECT_URI").expect("Missing REDIRECT_URI env var"),
                message_components_config_path: env::var("MESSAGE_COMPONENTS_CONFIG_PATH")
                    .expect("Missing MESSAGE_COMPONENTS_CONFIG_PATH env var"),
            }
        }

        pub(crate) fn with_se_api_host(&self, new: String) -> Self {
            AppConfig {
                se_api_host: new.clone(),
                ..self.clone()
            }
        }

        pub(crate) fn with_se_jwt(&self, new: String) -> Self {
            AppConfig {
                se_jwt: Some(new),
                ..self.clone()
            }
        }

        pub(crate) fn with_twitch_client_secret(&self, new: String) -> Self {
            AppConfig {
                twitch_client_secret: Some(new),
                ..self.clone()
            }
        }
    }
}

#[derive(Clone)]
struct AppState {
    config: AppConfig,
}

impl AppState {
    fn new(config: AppConfig) -> Self {
        AppState { config }
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
                println!("Failed to create auth token: {e}");
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::Empty)
                    .unwrap();
            }
        };

        let oauth_response = match resp.text().await {
            Ok(response) => response,
            Err(e) => {
                println!("Error getting response data from oauth API: {e}");
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::Empty)
                    .unwrap();
            }
        };

        match auth::securely_store_oauth_tokens(oauth_response).await {
            Ok(secret_name) => println!("Successfully stored in {secret_name}"),
            Err(e) => println!("Failed to store oauth response: {e}"),
        }

        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")
            .body(Body::from("Authorized! Have a nice day!"))
            .unwrap();
    }

    if let (Some(error), Some(error_desc)) = (params.get("error"), params.get("error_description"))
    {
        println!("Authorization denied by user :( Error: {error} and description: {error_desc}");

        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")
            .body(Body::from("Authorization denied :("))
            .unwrap();
    }

    println!("Authorization request from Twitch is missing code and/or scopes param");

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
    let event_handler = EventHandler::new(webclient);

    match event_handler.handle(body, &headers, &state.config).await {
        Ok(resp) => resp,
        Err(e) => {
            println!("Event handling failed with error: {}", e);

            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::Empty)
                .unwrap()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    println!("Hello, world!");

    let config = AppConfig::from_env();
    let state = AppState::new(config);

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

    use crate::config::AppConfig;

    #[test]
    fn from_env_creates_config() -> Result<()> {
        dotenvy::from_filename(".env.test")?;
        let _result = AppConfig::from_env();
        Ok(())
    }
}
