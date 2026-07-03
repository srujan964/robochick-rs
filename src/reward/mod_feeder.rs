use super::RewardHandler;
use crate::{
    client::StreamelementsCaller,
    config::AppConfig,
    robochick::twitch::{MessageBuilder, MessageComponents, Robochick},
    types::twitch::RewardRedeemed,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use fastrand::Rng;
use std::path::PathBuf;

pub struct ModFeed<C: StreamelementsCaller> {
    pub client: C,
}

#[async_trait]
impl<C: StreamelementsCaller> RewardHandler for ModFeed<C> {
    async fn handle(&self, redeem: &RewardRedeemed, config: &AppConfig) -> Result<()> {
        let msg_config_path = PathBuf::from(config.message_components_config_path.clone());
        let message_components: MessageComponents = match read_config(&msg_config_path) {
            Ok(m) => m,
            Err(e) => {
                println!("Error reading message configuration file: {e}");
                return Ok(());
            }
        };

        let mut rng: Rng = Rng::new();
        let message = match Robochick::build_from_templates(&message_components, &mut rng) {
            Ok(m) => m,
            Err(e) => {
                println!("Failed to build message: {e}");
                return Ok(());
            }
        };

        println!("Message built: {}", &message);
        return match self.client.say(&message, config).await {
            Ok(resp) => {
                println!("Successfully posted message in chat!");
                Ok(())
            }
            Err(e) => {
                println!("Streamelements API request failed: {e}");
                Ok(())
            }
        };
    }
}

fn read_config(path: &PathBuf) -> Result<MessageComponents> {
    let config_str = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => {
            return Err(anyhow!(
                "Failed to read configuration file for building messages"
            ));
        }
    };

    serde_json::from_str::<MessageComponents>(config_str.as_ref())
        .map_err(|e| anyhow!("Failed to deserialize message config: {e}"))
}

#[cfg(test)]
mod tests {
    use crate::client::StreamelementsCaller;
    use crate::config::AppConfig;
    use crate::reward::RewardHandler;
    use crate::reward::mod_feeder::ModFeed;
    use crate::types::twitch::{self, RewardRedeemed};
    use anyhow::Result;
    use axum::http::HeaderMap;
    use lambda_http::{Body, Response};
    use mockall::{mock, predicate};
    use reqwest::StatusCode;
    use std::path::PathBuf;

    mock! {
        pub Caller {}

        impl StreamelementsCaller for Caller {
            async fn say(&self, msg: &str, config: &AppConfig) -> Result<String>;
        }
    }

    #[tokio::test]
    async fn builds_scenario_and_calls_streamelements_api() -> Result<()> {
        dotenvy::from_filename(".env.test")?;
        let config = AppConfig::from_env();

        let mut payload_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        payload_path.push("resources/tests/reward_redemption_event.json");
        let payload = std::fs::read_to_string(payload_path)?;
        let event: RewardRedeemed = serde_json::from_str::<RewardRedeemed>(&payload)?;

        let expected_message =
            "Anna's feeling benevolent this time, all the mods got a dry cracker each!";

        let mut mock_caller = MockCaller::new();
        let se_mock = mock_caller
            .expect_say()
            .with(
                predicate::eq(expected_message.to_string()),
                predicate::eq(config.clone()),
            )
            .return_once(|_, _| Ok("result".to_string()))
            .once();

        let handler = ModFeed {
            client: mock_caller,
        };

        let response: Result<()> = handler.handle(&event, &config).await;

        assert!(response.is_ok());
        Ok(())
    }
}
