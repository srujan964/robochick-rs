mod client;
mod robochick;
mod types;

pub mod config {
    use std::env;

    #[derive(Clone)]
    pub struct AppConfig {
        pub twitch_client_id: String,
        pub twitch_client_secret: String,
        pub twitch_eventsub_subscription_secret: String,
        pub se_jwt: String,
        pub se_api_host: String,
        pub aws_session_token: String,
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
                se_jwt: env::var("SE_JWT").expect("Missing SE_JWT env var"),
                se_api_host: env::var("SE_API_HOST").expect("Missing SE_API_HOST env var"),
                aws_session_token: env::var("AWS_SESSION_TOKEN")
                    .expect("Missing AWS_SESSION_TOKEN env var"),
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

        let result = AppConfig::from_env();

        assert_eq!(result.se_api_host, "http://localhost:3000/streamelements/");
        assert_eq!(result.se_jwt, "se-token-value");
        assert_eq!(result.twitch_eventsub_subscription_secret, "chickencoop");
        assert_eq!(result.twitch_client_secret, "0123456789");
        assert_eq!(result.twitch_client_id, "client-id");

        Ok(())
    }
}
