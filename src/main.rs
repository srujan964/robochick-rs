mod client;
mod robochick;
mod types;

pub mod config {
    use std::env;

    #[derive(Clone, Debug)]
    pub struct AppConfig {
        pub twitch_client_id: String,
        pub twitch_client_secret: String,
        pub twitch_eventsub_subscription_secret: String,
        pub twitch_channel_id: String,
        pub se_jwt: String,
        pub se_api_host: String,
        pub aws_session_token: String,
        pub aws_parameter_store_host: String,
    }

    impl AppConfig {
        pub fn from_env() -> AppConfig {
            AppConfig {
                twitch_client_id: env::var("TWITCH_CLIENT_ID")
                    .expect("Missing TWITCH_CLIENT_ID env var"),
                twitch_client_secret: env::var("TWITCH_CLIENT_SECRET")
                    .expect("Missing TWITCH_CLIENT_SECRET env var"),
                twitch_eventsub_subscription_secret: env::var(
                    "TWITCH_EVENTSUB_SUBSCRIPTION_SECRET",
                )
                .expect("Missing TWITCH_EVENTSUB_SUBSCRIPTION_SECRET env var"),
                twitch_channel_id: env::var("TWITCH_CHANNEL_ID")
                    .expect("Missing TWITCH_CHANNEL_ID env var"),
                se_jwt: env::var("SE_JWT").expect("Missing SE_JWT env var"),
                se_api_host: env::var("SE_API_HOST").expect("Missing SE_API_HOST env var"),
                aws_session_token: env::var("AWS_SESSION_TOKEN")
                    .expect("Missing AWS_SESSION_TOKEN env var"),
                aws_parameter_store_host: env::var("AWS_PARAMETER_STORE_HOST")
                    .expect("Missing AWS_PARAMETER_STORE_HOST env var"),
            }
        }

        pub(crate) fn with_aws_parameter_store_host(&self, new: String) -> Self {
            AppConfig {
                aws_parameter_store_host: new.clone(),
                ..self.clone()
            }
        }

        pub(crate) fn with_se_api_host(&self, new: String) -> Self {
            AppConfig {
                se_api_host: new.clone(),
                ..self.clone()
            }
        }
    }
}

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use dotenvy::dotenv;

    use crate::config::AppConfig;

    #[test]
    fn from_env_creates_config() -> Result<()> {
        dotenv()?;
        let _result = AppConfig::from_env();
        Ok(())
    }
}
