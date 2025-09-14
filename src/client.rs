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

pub trait ParameterStoreCaller {
    async fn get<T: DeserializeOwned + 'static>(
        &self,
        path: String,
        params: Vec<(String, String)>,
        config: &AppConfig,
    ) -> Result<T>;
}

pub trait StreamelementsCaller {
    async fn say(&self, msg: String, config: &AppConfig) -> Result<String>;
}

impl StreamelementsCaller for WebClient {
    async fn say(&self, msg: String, config: &AppConfig) -> Result<String> {
        let host = config.se_api_host.clone();
        let mut url = Url::parse(&host)?;
        url = url.join(format!("/kappa/v2/bot/{}/say", &config.twitch_channel_id).as_ref())?;

        let req_body = MessageRequest {
            message: msg.clone(),
        };

        let req_body_string = serde_json::to_string(&req_body)?;

        if let Ok(resp) = self
            .client
            .post(url)
            .bearer_auth(&config.se_jwt)
            .body(Body::from(req_body_string))
            .timeout(Duration::new(1, 0))
            .send()
            .await
        {
            if resp.status().is_success() {
                match resp.text().await {
                    Ok(text) => Ok(text),
                    Err(_) => Err(anyhow!("Failed to read response body")),
                }
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

impl ParameterStoreCaller for WebClient {
    async fn get<T: DeserializeOwned>(
        &self,
        path: String,
        params: Vec<(String, String)>,
        config: &AppConfig,
    ) -> Result<T> {
        dbg!(config);

        let host = config.aws_parameter_store_host.clone();
        let mut url = Url::parse(&host)?;
        url = url.join(&path)?;

        if let Ok(resp) = self
            .client
            .get(url)
            .query(&params)
            .header(
                "X-Aws-Parameters-Secrets-Token",
                config.aws_session_token.clone(),
            )
            .timeout(Duration::new(1, 0))
            .send()
            .await
        {
            if resp.status().is_success() {
                resp.json::<T>()
                    .await
                    .map_err(|e| anyhow!("Error deserializing response: {e}"))
            } else {
                Err(anyhow!("Request to AWS Parameter Store"))
            }
        } else {
            Err(anyhow!("Error making request to AWS Parameter Store"))
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
        client::{ParameterStoreCaller, StreamelementsCaller, WebClient},
        config::AppConfig,
        robochick::twitch::MessageComponents,
    };

    #[tokio::test]
    async fn param_store_get_returns_response_deserialized() -> Result<()> {
        dotenv()?;
        let mut config = AppConfig::from_env();
        let mut mock_server = Server::new_async().await;
        config = config
            .with_aws_parameter_store_host(format!("http://{}", mock_server.host_with_port()));

        let mut resp_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        resp_path.push("resources/tests/message_components_param_value.json");

        let mock = mock_server
            .mock("GET", "/systemsmanager/parameters/get/")
            .match_query(mockito::Matcher::UrlEncoded(
                "name".into(),
                "message_components".into(),
            ))
            .match_header(
                "X-Aws-Parameters-Secrets-Token",
                mockito::Matcher::Exact(config.aws_session_token.clone()),
            )
            .with_body_from_file(resp_path)
            .with_header("Content-Type", "application/json")
            .create_async()
            .await;

        let client = Client::new();
        let webclient = WebClient::new(client);

        let path = "/systemsmanager/parameters/get/";
        let params = vec![("name".to_string(), "message_components".to_string())];
        let response = webclient
            .get::<MessageComponents>(path.to_string(), params, &config)
            .await?;

        assert_eq!(response.get_mods(), vec!["John", "Jane", "Alex", "Krish"]);
        assert_eq!(response.scenarios.len(), 3);

        mock.assert_async().await;
        Ok(())
    }

    #[tokio::test]
    async fn param_store_get_returns_err_on_deserialization_failure() -> Result<()> {
        dotenv()?;
        let mut config = AppConfig::from_env();
        let mut mock_server = Server::new_async().await;
        config = config
            .with_aws_parameter_store_host(format!("http://{}", mock_server.host_with_port()));

        let mock = mock_server
            .mock("GET", "/systemsmanager/parameters/get/")
            .match_query(mockito::Matcher::UrlEncoded(
                "name".into(),
                "message_components".into(),
            ))
            .match_header(
                "X-Aws-Parameters-Secrets-Token",
                mockito::Matcher::Exact(config.aws_session_token.clone()),
            )
            .with_body("invalid body")
            .with_header("Content-Type", "application/json")
            .create_async()
            .await;

        let client = Client::new();
        let webclient = WebClient::new(client);

        let path = "/systemsmanager/parameters/get/";
        let params = vec![("name".to_string(), "message_components".to_string())];
        let response = webclient
            .get::<MessageComponents>(path.to_string(), params, &config)
            .await;

        assert!(response.is_err());

        mock.assert_async().await;
        Ok(())
    }

    #[tokio::test]
    async fn param_store_get_returns_err_on_4xx() -> Result<()> {
        dotenv()?;
        let mut config = AppConfig::from_env();
        let mut mock_server = Server::new_async().await;
        config = config
            .with_aws_parameter_store_host(format!("http://{}", mock_server.host_with_port()));

        let mock = mock_server
            .mock("GET", "/systemsmanager/parameters/get/")
            .match_query(mockito::Matcher::UrlEncoded(
                "name".into(),
                "message_components".into(),
            ))
            .match_header(
                "X-Aws-Parameters-Secrets-Token",
                mockito::Matcher::Exact(config.aws_session_token.clone()),
            )
            .with_status(404)
            .create_async()
            .await;

        let client = Client::new();
        let webclient = WebClient::new(client);

        let path = "/systemsmanager/parameters/get/";
        let params = vec![("name".to_string(), "message_components".to_string())];
        let response = webclient
            .get::<MessageComponents>(path.to_string(), params, &config)
            .await;

        assert!(response.is_err());

        mock.assert_async().await;
        Ok(())
    }

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
            .mock("POST", "/kappa/v2/bot/example_channel_id/say")
            .match_header(
                "Authorization",
                mockito::Matcher::Exact(format!("Bearer {}", &config.se_jwt)),
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
            .mock("POST", "/kappa/v2/bot/example_channel_id/say")
            .match_header(
                "Authorization",
                mockito::Matcher::Exact(format!("Bearer {}", &config.se_jwt)),
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
