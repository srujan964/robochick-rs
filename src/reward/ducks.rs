use crate::{config::AppConfig, reward::RewardHandler, types::twitch::RewardRedeemed};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use aws_sdk_dynamodb::{Client, types::AttributeValue};

pub struct DuckRedeemed {
    pub dynamo_client: Client,
}

#[async_trait]
impl RewardHandler for DuckRedeemed {
    async fn handle(&self, redeem: &RewardRedeemed, config: &AppConfig) -> Result<()> {
        let username = redeem.event.username();
        let redemption_ts = redeem.event.redeemed_at();

        match self
            .dynamo_client
            .put_item()
            .table_name(config.duck_rewards_table_name.clone())
            .item("username", AttributeValue::S(username.to_string()))
            .item("redeemed_at", AttributeValue::S(redemption_ts.to_string()))
            .send()
            .await
        {
            Ok(_) => Ok(()),
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
        operation::put_item::{PutItemOutput, builders::PutItemOutputBuilder},
        types::AttributeValue,
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

        let mut expected_attrs: HashMap<String, AttributeValue> = HashMap::new();
        expected_attrs.insert(
            "username".to_string(),
            AttributeValue::S(redemption.event.username().to_string()),
        );
        expected_attrs.insert(
            "redeemed_at".to_string(),
            AttributeValue::S(redemption.event.redeemed_at().to_string()),
        );
        let expected_table_name = config.duck_rewards_table_name.clone();

        let put_req_rule: Rule = mock!(Client::put_item)
            .match_requests(move |r| {
                r.table_name == Some(expected_table_name.clone())
                    && r.item == Some(expected_attrs.clone())
            })
            .then_output(|| PutItemOutput::builder().build());

        let dynamo_client = mock_client!(aws_sdk_dynamodb, [&put_req_rule]);

        let handler = DuckRedeemed { dynamo_client };
        let resp = handler.handle(&redemption, &config).await;

        assert!(resp.is_ok());
        assert_eq!(put_req_rule.num_calls(), 1);
        Ok(())
    }
}
