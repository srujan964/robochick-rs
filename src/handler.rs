pub mod event_handler {
    use std::{path::PathBuf, str::FromStr};

    use anyhow::{Result, anyhow};
    use axum::http::{HeaderMap, HeaderName};
    use fastrand::Rng;
    use hex::decode;
    use hmac::{Hmac, Mac};
    use lambda_http::{Body, Response, tracing};
    use reqwest::{
        StatusCode,
        header::{self, CONTENT_TYPE},
    };
    use serde_json::Value;
    use sha2::Sha256;

    use crate::{
        client::StreamelementsCaller,
        config::AppConfig,
        robochick::twitch::{MessageBuilder, MessageComponents, Robochick},
        types::twitch::{
            EventsubHeader, MessageType, RevocationEvent, RewardRedeemed, SubscriptionType,
            VerificationEvent,
        },
    };

    type HmacSha256 = Hmac<Sha256>;

    pub struct EventHandler<T: StreamelementsCaller> {
        caller: T,
    }

    impl<T: StreamelementsCaller> EventHandler<T> {
        pub fn new(caller: T) -> Self {
            EventHandler { caller }
        }

        fn handle_challenge(
            payload: &str,
            headers: &HeaderMap,
            config: &AppConfig,
        ) -> Result<String> {
            let challenge_event = match serde_json::from_str::<VerificationEvent>(payload) {
                Ok(val) => val,
                Err(_) => {
                    return Err(anyhow!(
                        "Failed to deserialize payload to a VerificationEvent"
                    ));
                }
            };

            Ok(challenge_event.challenge().to_string())
        }

        fn handle_revocation(payload: &str, headers: &HeaderMap, config: &AppConfig) {
            if let Ok(event) = serde_json::from_str::<RevocationEvent>(payload) {
                println!(
                    "Subscription revoked for {} with reason: {}",
                    event.subscription_type(),
                    event.subscription_status()
                );
            } else {
                println!("Failed to parse payload");
            }
        }

        async fn handle_notification(
            &self,
            payload: &str,
            headers: &HeaderMap,
            config: &AppConfig,
        ) -> Result<()> {
            if let Some(header) = headers.get(EventsubHeader::SubscriptionType.as_ref()) {
                if header.to_str().map(SubscriptionType::from_str).is_err() {
                    return Err(anyhow!("Unknown Subscription-Type header: {:?}", header));
                }

                let event = match serde_json::from_str::<RewardRedeemed>(payload) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("Failed to deserialize event to RewardRedeemed type: {e}");
                        return Err(anyhow!("{e}"));
                    }
                };

                if event.broadcaster_user_id() != config.broadcaster_user_id
                    || event.reward_id() != config.feed_mods_rewards_id
                {
                    println!(
                        "Invalid notification: unknown broadcaster user id {} or reward id {}",
                        event.broadcaster_user_id(),
                        event.reward_id(),
                    );
                    return Err(anyhow!("Unknown notification"));
                }

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
                return match self.caller.say(&message, config).await {
                    Ok(resp) => {
                        println!("Successfully posted message in chat!");
                        Ok(())
                    }
                    Err(e) => {
                        println!("Streamelements API request failed: {e}");
                        Ok(())
                    }
                };
            } else {
                Err(anyhow!(
                    "Missing {} header",
                    EventsubHeader::SubscriptionType.as_ref()
                ))
            }
        }

        pub async fn handle(
            &self,
            request: String,
            headers: &HeaderMap,
            config: &AppConfig,
        ) -> Result<Response<Body>> {
            // fail early if we fail to verify if the event is from twitch or not

            match EventHandler::<T>::verify(&request, headers, config) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Unverified event. Error: {e}");
                    let resp = Response::builder()
                        .status(StatusCode::FORBIDDEN)
                        .body(Body::Empty)
                        .map_err(Box::new)?;

                    return Ok(resp);
                }
            }

            let message_type_val = match headers.get(EventsubHeader::MessageType.as_ref()) {
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

            let message_type = match MessageType::from_str(message_type_val) {
                Ok(s) => s,
                Err(_) => return Err(anyhow!("Invalid MessageType received")),
            };

            let resp: Response<Body> = match message_type {
                MessageType::WebhookCallbackVerification => {
                    if let Ok(challenge) =
                        EventHandler::<T>::handle_challenge(&request, headers, config)
                    {
                        println!("Responding to challenge request with: {challenge}");

                        Response::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_TYPE, "text/plain")
                            .body(Body::from(challenge))
                            .map_err(Box::new)?
                    } else {
                        Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::Empty)
                            .map_err(Box::new)?
                    }
                }

                MessageType::Notification => {
                    if self
                        .handle_notification(&request, headers, config)
                        .await
                        .is_ok()
                    {
                        Response::builder()
                            .status(StatusCode::NO_CONTENT)
                            .body(Body::Empty)
                            .map_err(Box::new)?
                    } else {
                        Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::Empty)
                            .map_err(Box::new)?
                    }
                }
                MessageType::Revocation => {
                    EventHandler::<T>::handle_revocation(&request, headers, config);

                    Response::builder()
                        .status(StatusCode::NO_CONTENT)
                        .body(Body::Empty)
                        .map_err(Box::new)?
                }
            };

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
        use anyhow::Result;
        use axum::http::{HeaderMap, Request};
        use core::time;
        use dotenvy::dotenv;
        use pretty_assertions::assert_eq;
        use reqwest::header::CONTENT_TYPE;
        use std::path::PathBuf;

        use hmac::{Hmac, Mac};
        use lambda_http::{Body, Response};
        use mockall::{mock, predicate};
        use reqwest::StatusCode;
        use sha2::Sha256;

        use crate::client::StreamelementsCaller;
        use crate::config::AppConfig;
        use crate::handler::event_handler::{self, EventHandler, HmacSha256};
        use crate::robochick::twitch::{MessageComponents, Scenario};
        use crate::types::twitch;

        mock! {
            pub Caller {}

            impl StreamelementsCaller for Caller {
                async fn say(&self, msg: &str, config: &AppConfig) -> Result<String>;
            }
        }

        #[test]
        fn verify_returns_true_for_valid_event() -> Result<()> {
            dotenvy::from_filename(".env.test")?;
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
            dotenvy::from_filename(".env.test")?;
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
            dotenvy::from_filename(".env.test")?;
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

        #[tokio::test]
        async fn handle_returns_challenge_string_in_plaintext() -> Result<()> {
            dotenvy::from_filename(".env.test")?;
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

            let response: Response<Body> = event_handler
                .handle(payload.to_string(), &headers, &config)
                .await?;

            assert_eq!(StatusCode::OK, response.status());
            assert_eq!("text/plain", response.headers().get(CONTENT_TYPE).unwrap());

            match response.body() {
                Body::Text(s) => assert_eq!(s, expected_challenge_val),
                Body::Binary(_) => panic!(),
                Body::Empty => panic!(),
            }

            Ok(())
        }

        #[tokio::test]
        async fn handle_returns_204_for_subscription_revocation() -> Result<()> {
            dotenvy::from_filename(".env.test")?;
            let config = AppConfig::from_env();
            let message_id = "message-1";
            let timestamp = "2025-09-14T00:00:00.123456789";

            let mut payload_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            payload_path.push("resources/tests/subscription_revoked.json");
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
                twitch::MessageType::Revocation.as_ref().parse().unwrap(),
            );

            let mock_caller = MockCaller::new();
            let event_handler = EventHandler::new(mock_caller);

            let response: Response<Body> = event_handler
                .handle(payload.to_string(), &headers, &config)
                .await?;

            assert_eq!(StatusCode::NO_CONTENT, response.status());
            Ok(())
        }

        #[tokio::test]
        async fn handle_builds_scenario_and_calls_streamelements_api() -> Result<()> {
            dotenvy::from_filename(".env.test")?;
            let config = AppConfig::from_env();
            let message_id = "message-1";
            let timestamp = "2025-09-14T00:00:00.123456789";

            let mut payload_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            payload_path.push("resources/tests/reward_redemption_event.json");
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
                twitch::MessageType::Notification.as_ref().parse().unwrap(),
            );
            headers.append(
                twitch::EventsubHeader::SubscriptionType.as_ref(),
                twitch::SubscriptionType::CustomRewardRedemption
                    .as_ref()
                    .parse()
                    .unwrap(),
            );

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

            let event_handler = EventHandler::new(mock_caller);

            let response: Response<Body> = event_handler
                .handle(payload.to_string(), &headers, &config)
                .await?;

            assert_eq!(StatusCode::NO_CONTENT, response.status());

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
