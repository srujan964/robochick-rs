pub mod event_handler {
    use anyhow::{Result, anyhow};
    use axum::http::{HeaderMap, HeaderName};
    use hex::decode;
    use hmac::{Hmac, Mac};
    use lambda_http::{Body, Response};
    use reqwest::{
        StatusCode,
        header::{self, CONTENT_TYPE},
    };
    use serde_json::Value;
    use sha2::Sha256;

    use crate::{
        client::{ParameterStoreCaller, StreamelementsCaller},
        config::AppConfig,
        types::twitch::{EventsubHeader, MessageType, SubscriptionVerficationEvent},
    };

    type HmacSha256 = Hmac<Sha256>;

    pub struct EventHandler<T: ParameterStoreCaller + StreamelementsCaller> {
        caller: T,
    }

    impl<T: ParameterStoreCaller + StreamelementsCaller> EventHandler<T> {
        pub fn new(caller: T) -> Self {
            EventHandler { caller }
        }

        fn handle_challenge(
            payload: String,
            headers: &HeaderMap,
            config: &AppConfig,
        ) -> Result<String> {
            let challenge_event =
                match serde_json::from_str::<SubscriptionVerficationEvent>(&payload) {
                    Ok(val) => val,
                    Err(_) => {
                        return Err(anyhow!("Failed to deserialize payload to serde json value"));
                    }
                };

            Ok(challenge_event.challenge().to_string())
        }

        pub fn handle(
            &self,
            request: String,
            headers: &HeaderMap,
            config: &AppConfig,
        ) -> Result<Response<Body>> {
            // fail early if we fail to verify if the event is from twitch or not
            if EventHandler::<T>::verify(&request, headers, config).is_err() {
                let resp = Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Body::Empty)
                    .map_err(Box::new)?;

                return Ok(resp);
            }

            let msg_type = match headers.get(EventsubHeader::MessageType.as_ref()) {
                Some(x) => match x.to_str() {
                    Ok(s) => s,
                    Err(_) => {
                        return Err(anyhow!(
                            "Failed to extract MessageType header val as a String"
                        ));
                    }
                },
                None => return Err(anyhow!("Missing MessageType header")),
            };

            let resp: Response<Body>;

            if msg_type == MessageType::WebhookCallbackVerification.as_ref() {
                match EventHandler::<T>::handle_challenge(request, headers, config) {
                    Ok(challenge) => {
                        resp = Response::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_TYPE, "text/plain")
                            .body(Body::from(challenge))
                            .map_err(Box::new)?;
                    }
                    Err(_) => {
                        resp = Response::builder()
                            .status(StatusCode::FORBIDDEN)
                            .body(Body::Empty)
                            .map_err(Box::new)?;
                    }
                }
            } else {
                resp = Response::builder().body(Body::Empty).map_err(Box::new)?;
            }

            Ok(resp)
        }

        fn verify(payload: &str, headers: &HeaderMap, config: &AppConfig) -> Result<()> {
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
        use anyhow::Result;
        use axum::http::{HeaderMap, Request};
        use core::time;
        use dotenvy::dotenv;
        use pretty_assertions::assert_eq;
        use reqwest::header::CONTENT_TYPE;
        use std::path::PathBuf;

        use hmac::{Hmac, Mac};
        use lambda_http::{Body, Response};
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

            let result = EventHandler::<MockCaller>::verify(payload, &headers, &config);

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
                EventHandler::<MockCaller>::verify(payload, &headers_without_msg_id, &config);

            assert!(result.is_err());
            Ok(())
        }

        #[test]
        fn verify_returns_false_for_incorrect_signature() -> Result<()> {
            dotenv()?;
            let config = AppConfig::from_env();
            let message_id = "message-1";
            let timestamp = "2025-09-14T00:00:00.123456789";
            let payload = r#"{"message":"Hello, World!"}"#;

            let input = format!("{}{}", timestamp.to_string(), payload.to_string());
            let signature = hex::encode("random data");

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

            let result = EventHandler::<MockCaller>::verify(payload, &headers, &config);

            assert!(result.is_err());
            Ok(())
        }

        #[test]
        fn handle_returns_challenge_string_in_plaintext() -> Result<()> {
            dotenv()?;
            let config = AppConfig::from_env();
            let message_id = "message-1";
            let timestamp = "2025-09-14T00:00:00.123456789";

            let mut payload_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            payload_path.push("resources/tests/challenge_request.json");
            let payload = std::fs::read_to_string(payload_path)?;

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
            headers.append(
                twitch::EventsubHeader::MessageType.as_ref(),
                twitch::MessageType::WebhookCallbackVerification
                    .as_ref()
                    .parse()
                    .unwrap(),
            );

            let expected_challenge_val = "pogchamp-kappa-360noscope-vohiyo";

            let mock_caller = MockCaller::new();
            let event_handler = EventHandler::new(mock_caller);

            let response: Response<Body> =
                event_handler.handle(payload.to_string(), &headers, &config)?;

            assert_eq!(StatusCode::OK, response.status());
            assert_eq!("text/plain", response.headers().get(CONTENT_TYPE).unwrap());

            match response.body() {
                Body::Text(s) => assert_eq!(s, expected_challenge_val),
                Body::Binary(_) => panic!(),
                Body::Empty => panic!(),
            }

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
