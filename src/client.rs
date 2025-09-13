use std::{fmt::Debug, time::Duration};

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
    async fn get<T: DeserializeOwned + Debug>(
        &self,
        path: String,
        params: Vec<(String, String)>,
        config: &AppConfig,
    ) -> Result<T>;
}

impl ParameterStoreCaller for WebClient {
    async fn get<T: DeserializeOwned + Debug>(
        &self,
        path: String,
        params: Vec<(String, String)>,
        config: &AppConfig,
    ) -> Result<T> {
        let mut url = Url::parse("http://localhost:2773")?;
        url = url.join(&path)?;

        match self
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
            Ok(resp) => resp
                .json::<T>()
                .await
                .map_err(|e| anyhow!("Error deserializing response: {e}")),
            Err(e) => Err(anyhow!("Error making request: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use dotenvy::dotenv;
    use mockito::{Server, ServerOpts};
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
        let config = AppConfig::from_env();
        let server_opts = ServerOpts {
            host: "127.0.0.1",
            port: 2773,
            assert_on_drop: false,
        };
        let mut mock_server = Server::new_with_opts_async(server_opts).await;

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
            // .with_body(response_body)
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
}
