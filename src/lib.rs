pub mod auth;
pub mod client;
pub mod handler;
pub mod reward;
pub mod robochick;
pub mod types;

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
        pub templates_config_path: String,
        pub rubberduck_rewards_id: String,
        pub duck_rewards_table_name: String,
        pub auth_secret_name: String,
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
                templates_config_path: env::var("TEMPLATES_CONFIG_PATH")
                    .expect("Missing TEMPLATES_CONFIG_PATH env var"),
                rubberduck_rewards_id: env::var("RUBBERDUCK_REWARD_ID")
                    .expect("Missing RUBBERDUCK_REWARD_ID env var"),
                duck_rewards_table_name: env::var("DUCK_REWARDS_TABLE_NAME")
                    .expect("Missing DUCK_REWARDS_TABLE_NAME env var"),
                auth_secret_name: env::var("AUTH_SECRET_NAME")
                    .expect("Missing AUTH_SECRET_NAME env var"),
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
