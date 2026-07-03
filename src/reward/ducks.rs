use crate::{config::AppConfig, reward::RewardHandler, types::twitch::RewardRedeemed};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use aws_sdk_dynamodb::{Client, error::SdkError, types::AttributeValue};

pub struct DuckRedeemed {
    pub dynamo_client: Client,
}

#[async_trait]
impl RewardHandler for DuckRedeemed {
    async fn handle(
        &self,
        msg_id: String,
        redeem: &RewardRedeemed,
        config: &AppConfig,
    ) -> Result<()> {
        let username = redeem.event.username();
        let redemption_ts = redeem.event.redeemed_at();
        let now_ts = chrono::Utc::now().to_rfc3339();

        match self
            .dynamo_client
            .put_item()
            .table_name(config.duck_rewards_table_name.clone())
            .item("message_id", AttributeValue::S(msg_id.clone()))
            .item("username", AttributeValue::S(username.to_string()))
            .item("redeemed_at", AttributeValue::S(redemption_ts.to_string()))
            .item("processed_at", AttributeValue::S(now_ts))
            .condition_expression("attribute_not_exists(message_id)")
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(SdkError::ServiceError(e)) if e.err().is_conditional_check_failed_exception() => {
                println!(
                    "A record with this message-id {} already exists, ignoring.",
                    msg_id
                );
                Ok(())
            }
            Err(_) => Err(anyhow!("Failed to insert duck redeem to table")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::AppConfig, reward::RewardHandler, reward::ducks::DuckRedeemed,
        types::twitch::RewardRedeemed,
    };
    use anyhow::Result;
    use aws_sdk_dynamodb::{
        Client,
        operation::put_item::{PutItemError, PutItemOutput, builders::PutItemOutputBuilder},
        types::{AttributeValue, error::ConditionalCheckFailedException},
    };
    use aws_smithy_mocks::{Rule, RuleMode, mock, mock_client};
    use pretty_assertions::assert_eq;
    use std::{collections::HashMap, path::PathBuf};

    #[tokio::test]
    async fn write_to_table() -> Result<()> {
        dotenvy::from_filename(".env.test")?;
        let config = AppConfig::from_env();

        let mut payload_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        payload_path.push("resources/tests/reward_redemption_event.json");
        let payload = std::fs::read_to_string(payload_path)?;
        let redemption: RewardRedeemed = serde_json::from_str::<RewardRedeemed>(&payload)?;
        let expected_username = redemption.event.username().to_string();
        let expected_redeemed_at = redemption.event.redeemed_at().to_string();

        let mut expected_attrs: HashMap<String, AttributeValue> = HashMap::new();
        let msg_id = String::from("Message-Id");

        let put_req_rule: Rule = mock!(Client::put_item)
            .match_requests(move |r| {
                let attr =
                    |k: &str| -> Option<&str> { r.item()?.get(k)?.as_s().ok().map(|s| s.as_str()) };

                attr("message_id") == Some("Message-Id")
                    && attr("username") == Some(&expected_username)
                    && attr("redeemed_at") == Some(&expected_redeemed_at)
                    && attr("processed_at")
                        .map(|d| chrono::DateTime::parse_from_rfc3339(d).is_ok())
                        .unwrap_or(false)
            })
            .then_output(|| PutItemOutput::builder().build());

        let dynamo_client = mock_client!(aws_sdk_dynamodb, [&put_req_rule]);

        let handler = DuckRedeemed { dynamo_client };
        let resp = handler.handle(msg_id, &redemption, &config).await;

        assert!(resp.is_ok());
        assert_eq!(put_req_rule.num_calls(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn ignore_duplicate_message_id() -> Result<()> {
        dotenvy::from_filename(".env.test")?;
        let config = AppConfig::from_env();

        let mut payload_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        payload_path.push("resources/tests/reward_redemption_event.json");
        let payload = std::fs::read_to_string(payload_path)?;
        let redemption: RewardRedeemed = serde_json::from_str::<RewardRedeemed>(&payload)?;

        let mut expected_attrs: HashMap<String, AttributeValue> = HashMap::new();
        let msg_id = String::from("Dup-Message-Id");
        let expected_table_name = config.duck_rewards_table_name.clone();

        let put_req_rule: Rule = mock!(Client::put_item)
            .match_requests(move |r| r.table_name == Some(expected_table_name.clone()))
            .then_error(|| {
                PutItemError::ConditionalCheckFailedException(
                    ConditionalCheckFailedException::builder()
                        .message("The request failed")
                        .build(),
                )
            });

        let dynamo_client = mock_client!(aws_sdk_dynamodb, [&put_req_rule]);

        let handler = DuckRedeemed { dynamo_client };
        let resp = handler.handle(msg_id, &redemption, &config).await;

        assert!(resp.is_ok());
        assert_eq!(put_req_rule.num_calls(), 1);
        Ok(())
    }
}
