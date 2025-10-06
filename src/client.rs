use std::time::Duration;

use anyhow::{Result, anyhow};
use reqwest::{Body, Client, Url, header::AUTHORIZATION};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::config::AppConfig;

pub struct WebClient {
    client: Client,
}

impl WebClient {
    pub fn new(client: Client) -> WebClient {
        WebClient { client }
    }
}

#[derive(Serialize, Deserialize)]
struct MessageRequest {
    message: String,
}

pub trait StreamelementsCaller {
    async fn say(&self, msg: String, config: &AppConfig) -> Result<String>;
}

impl StreamelementsCaller for WebClient {
    async fn say(&self, msg: String, config: &AppConfig) -> Result<String> {
        let host = config.se_api_host.clone();
        let mut url = Url::parse(&host)?;
        url = url.join(format!("kappa/v2/bot/{}/say", &config.twitch_channel_id).as_ref())?;

        let req_body = MessageRequest {
            message: msg.clone(),
        };

        let req_body_string = serde_json::to_string(&req_body)?;

        if let Ok(resp) = self
            .client
            .post(url)
            .bearer_auth(config.se_jwt.as_ref().expect("Missing Streamelements JWT"))
            .body(Body::from(req_body_string))
            .timeout(Duration::new(1, 0))
            .send()
            .await
        {
            if resp.status().is_success() {
                resp.text()
                    .await
                    .map_err(|e| anyhow!("Failed to read response body"))
            } else {
                Err(anyhow!(
                    "Streamelemenst API returned error with status: {}",
                    resp.status()
                ))
            }
        } else {
            Err(anyhow!("Failed to make request to Streamelements API"))
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use dotenvy::dotenv;
    use mockito::Server;
    use reqwest::Client;
    use std::path::PathBuf;

    use crate::{
        client::{StreamelementsCaller, WebClient},
        config::AppConfig,
        robochick::twitch::MessageComponents,
    };

    #[tokio::test]
    async fn say_makes_successful_request() -> Result<()> {
        dotenv()?;
        let mut config = AppConfig::from_env();
        let mut mock_server = Server::new_async().await;
        config = config.with_se_api_host(format!("http://{}", mock_server.host_with_port()));

        let response_body = r#"{
            "status":200,
            "channel":"example_channel_id",
            "message":"Hello, World!",
        }"#;

        let expected_body = r#"{"message":"Hello, World!"}"#;
        let mock = mock_server
            .mock("POST", "/kappa/v2/bot/test_channel_id/say")
            .match_header(
                "Authorization",
                mockito::Matcher::Exact(format!("Bearer {}", &config.se_jwt.as_ref().unwrap())),
            )
            .match_body(expected_body)
            .with_body(&response_body)
            .create_async()
            .await;

        let client = Client::new();
        let webclient = WebClient::new(client);

        let message: String = "Hello, World!".into();
        let result = webclient.say(message, &config).await?;

        mock.assert_async().await;
        assert_eq!(result, response_body);
        Ok(())
    }

    #[tokio::test]
    async fn say_returns_err_if_api_returns_4xx_error() -> Result<()> {
        dotenv()?;
        let mut config = AppConfig::from_env();
        let mut mock_server = Server::new_async().await;
        config = config.with_se_api_host(format!("http://{}", mock_server.host_with_port()));

        let response_body = r#"{
            "status":200,
            "channel":"example_channel_id",
            "message":"Hello, World!",
        }"#;

        let expected_body = r#"{"message":"Hello, World!"}"#;
        let mock = mock_server
            .mock("POST", "/kappa/v2/bot/test_channel_id/say")
            .match_header(
                "Authorization",
                mockito::Matcher::Exact(format!("Bearer {}", &config.se_jwt.as_ref().unwrap())),
            )
            .match_body(expected_body)
            .with_status(400)
            .create_async()
            .await;

        let client = Client::new();
        let webclient = WebClient::new(client);

        let message: String = "Hello, World!".into();
        let result = webclient.say(message, &config).await;

        mock.assert_async().await;
        assert!(result.is_err());
        Ok(())
    }
}
