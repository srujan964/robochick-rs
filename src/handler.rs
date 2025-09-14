pub mod event_handler {
    use anyhow::{Result, anyhow};
    use axum::http::HeaderMap;
    use hmac::{Hmac, Mac};
    use lambda_http::{Body, Response};
    use reqwest::StatusCode;
    use sha2::Sha256;

    use crate::{
        client::{ParameterStoreCaller, StreamelementsCaller},
        config::AppConfig,
        types::twitch::EventsubHeader,
    };

    type HmacSha256 = Hmac<Sha256>;

    pub struct EventHandler<T: ParameterStoreCaller + StreamelementsCaller> {
        caller: T,
    }

    impl<T: ParameterStoreCaller + StreamelementsCaller> EventHandler<T> {
        pub fn new(caller: T) -> Self {
            EventHandler { caller }
        }

        fn verify(&self, payload: String, headers: &HeaderMap, config: &AppConfig) -> bool {
            let message_id = headers
                .get(EventsubHeader::MessageId.as_ref())
                .unwrap()
                .to_str()
                .unwrap();

            let timestamp = headers
                .get(EventsubHeader::MessageTimestamp.as_ref())
                .unwrap()
                .to_str()
                .unwrap();

            let signature_val = headers
                .get(EventsubHeader::MessageSignature.as_ref())
                .unwrap()
                .to_str()
                .unwrap();

            let plaintext = format!("{}{}{}", message_id, timestamp, payload);

            let key = config.twitch_eventsub_subscription_secret.as_bytes();
            let hmac = HmacSha256::new_from_slice(key).unwrap();

            let signature = String::from(signature_val).strip_prefix("sha256=").unwrap();

            hmac.verify_slice(signature_val.as_bytes()).is_err()
        }
    }

    #[cfg(test)]
    mod tests {
        use core::time;

        use anyhow::Result;
        use axum::http::{HeaderMap, Request};
        use dotenvy::dotenv;

        use hmac::{Hmac, Mac};
        use mockall::mock;
        use reqwest::StatusCode;
        use sha2::Sha256;

        use crate::client::{ParameterStoreCaller, StreamelementsCaller};
        use crate::config::AppConfig;
        use crate::handler::event_handler::{self, EventHandler, HmacSha256};
        use crate::types::twitch;

        mock! {
            pub Caller {}

            impl ParameterStoreCaller for Caller {
                async fn get<MessageComponents: 'static>(&self, path: String, params: Vec<(String, String)>, config: &AppConfig) -> Result<MessageComponents>;
            }

            impl StreamelementsCaller for Caller {
                async fn say(&self, msg: String, config: &AppConfig) -> Result<String>;
            }
        }

        #[test]
        fn handle_should_process_a_verification_event() -> Result<()> {
            dotenv()?;
            let config = AppConfig::from_env();
            let message_id = "message-1";
            let timestamp = "2025-09-14T00:00:00.123456789";
            let payload = r#"{"message":"Hello, World!"}"#;

            let input = format!(
                "{}{}{}",
                message_id.to_string(),
                timestamp.to_string(),
                payload.to_string()
            );
            let signature = generate_hmac(&input, &config.twitch_eventsub_subscription_secret)?;

            let mut headers = HeaderMap::new();
            headers.append(
                twitch::EventsubHeader::MessageId.as_ref(),
                message_id.parse().unwrap(),
            );
            headers.append(
                twitch::EventsubHeader::MessageTimestamp.as_ref(),
                timestamp.parse().unwrap(),
            );
            headers.append(
                twitch::EventsubHeader::MessageSignature.as_ref(),
                signature.parse().unwrap(),
            );

            let mock_caller = MockCaller::new();
            let event_handler = EventHandler::new(mock_caller);

            let result = event_handler.verify(payload.to_string(), &headers, &config);

            assert!(result);
            Ok(())
        }

        fn generate_hmac(input: &str, secret: &str) -> Result<String> {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())?;
            mac.update(input.as_bytes());
            let hmac = mac.finalize();

            let encoded_hmac = hex::encode(hmac.into_bytes());
            Ok(format!("sha256={}", &encoded_hmac))
        }
    }
}
