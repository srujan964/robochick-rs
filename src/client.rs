use std::time::Duration;

use anyhow::{Result, anyhow};
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;

use crate::config::AppConfig;

pub struct WebClient {
    client: Client,
}

impl WebClient {
    pub fn new(client: Client) -> WebClient {
        WebClient { client }
    }
}

trait ParameterStoreCaller {
    async fn get<T: DeserializeOwned>(
        &self,
        path: String,
        params: Vec<(String, String)>,
        config: &AppConfig,
    ) -> Result<T>;
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
            .timeout(Duration::new(2, 0))
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
        client::{ParameterStoreCaller, WebClient},
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
}
