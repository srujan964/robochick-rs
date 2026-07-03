use anyhow::Result;
use async_trait::async_trait;

use crate::{config::AppConfig, types::twitch::RewardRedeemed};

pub mod ducks;
pub mod mod_feeder;

#[async_trait]
pub trait RewardHandler: Send + Sync {
    async fn handle(&self, redeem: &RewardRedeemed, config: &AppConfig) -> Result<()>;
}
