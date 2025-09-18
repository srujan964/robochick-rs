pub mod event_handler {
    use anyhow::{Result, anyhow};
    use axum::http::HeaderMap;
    use hex::decode;
    use hmac::{Hmac, Mac};
    use lambda_http::{Body, Response};
    use reqwest::{StatusCode, header};
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

        fn verify(&self, payload: String, headers: &HeaderMap, config: &AppConfig) -> Result<()> {
            if let (Some(message_id), Some(timestamp), Some(signature_val)) = (
                headers.get(EventsubHeader::MessageId.as_ref()),
                headers.get(EventsubHeader::MessageTimestamp.as_ref()),
                headers.get(EventsubHeader::MessageSignature.as_ref()),
            ) {
                if let (Ok(message_id_val), Ok(timestamp_val), Ok(signature_val)) = (
                    message_id.to_str(),
                    timestamp.to_str(),
                    signature_val.to_str(),
                ) {
                    let input = format!("{}{}{}", message_id_val, timestamp_val, payload);

                    let key = config.twitch_eventsub_subscription_secret.as_bytes();
                    let mut hmac = HmacSha256::new_from_slice(key)?;
                    hmac.update(input.as_bytes());

                    let signature = match String::from(signature_val).strip_prefix("sha256=") {
                        Some(s) => hex::decode(s)?,
                        None => {
                            return Err(anyhow!(
                                "Failed to strip `sha256=` prefix from signature header"
                            ));
                        }
                    };

                    match hmac.verify_slice(&signature[..]) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(anyhow!("Signature verification failed: {e}")),
                    }
                } else {
                    Err(anyhow!("Failed to parse headers to strings"))
                }
            } else {
                Err(anyhow!(
                    "Missing one of these headers: Message-Id, Message-Timestamp, Message-Signature"
                ))
            }
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
        fn verify_returns_true_for_valid_event() -> Result<()> {
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

            dbg!(&result);
            assert!(result.is_ok());
            Ok(())
        }

        #[test]
        fn verify_returns_false_for_missing_header() -> Result<()> {
            dotenv()?;
            let config = AppConfig::from_env();
            let timestamp = "2025-09-14T00:00:00.123456789";
            let payload = r#"{"message":"Hello, World!"}"#;

            let input = format!("{}{}", timestamp.to_string(), payload.to_string());
            let signature = generate_hmac(&input, &config.twitch_eventsub_subscription_secret)?;

            let mut headers_without_msg_id = HeaderMap::new();
            headers_without_msg_id.append(
                twitch::EventsubHeader::MessageTimestamp.as_ref(),
                timestamp.parse().unwrap(),
            );
            headers_without_msg_id.append(
                twitch::EventsubHeader::MessageSignature.as_ref(),
                signature.parse().unwrap(),
            );

            let mock_caller = MockCaller::new();
            let event_handler = EventHandler::new(mock_caller);

            let result =
                event_handler.verify(payload.to_string(), &headers_without_msg_id, &config);

            assert!(result.is_err());
            Ok(())
        }


        #[test]
        fn verify_returns_false_for_incorrect_signature() -> Result<()> {
            dotenv()?;
            let config = AppConfig::from_env();
            let timestamp = "2025-09-14T00:00:00.123456789";
            let payload = r#"{"message":"Hello, World!"}"#;

            let input = format!("{}{}", timestamp.to_string(), payload.to_string());
            let signature = hex::encode("random data");

            let mut headers_without_msg_id = HeaderMap::new();
            headers_without_msg_id.append(
                twitch::EventsubHeader::MessageTimestamp.as_ref(),
                timestamp.parse().unwrap(),
            );
            headers_without_msg_id.append(
                twitch::EventsubHeader::MessageSignature.as_ref(),
                signature.parse().unwrap(),
            );

            let mock_caller = MockCaller::new();
            let event_handler = EventHandler::new(mock_caller);

            let result =
                event_handler.verify(payload.to_string(), &headers_without_msg_id, &config);

            assert!(result.is_err());
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
