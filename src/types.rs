pub mod twitch {
    use serde::{Deserialize, Serialize};
    use strum::AsRefStr;

    #[derive(Debug, AsRefStr)]
    pub enum EventsubHeader {
        #[strum(serialize = "Twitch-Eventsub-Message-Id")]
        MessageId,
        #[strum(serialize = "Twitch-Eventsub-Message-Retry")]
        MessageRetry,
        #[strum(serialize = "Twitch-Eventsub-Message-Type")]
        MessageType,
        #[strum(serialize = "Twitch-Eventsub-Message-Signature")]
        MessageSignature,
        #[strum(serialize = "Twitch-Eventsub-Message-Timestamp")]
        MessageTimestamp,
        #[strum(serialize = "Twitch-Eventsub-Subscription-Type")]
        SubscriptionType,
        #[strum(serialize = "Twitch-Eventsub-Subscription-Version")]
        SubscriptionVersion,
    }

    #[derive(Debug, AsRefStr)]
    pub enum MessageType {
        #[strum(serialize = "webhook_callback_verification")]
        WebhookCallbackVerification,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct RewardRedemptionEvent {
        pub(crate) subscription: Subscription,
        pub(crate) event: RewardEvent,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Subscription {
        id: String,
        r#type: String,
        version: String,
        status: String,
        cost: u16,
        condition: SubCondition,
        transport: SubscriptionTransport,
        created_at: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct RewardEvent {
        id: String,
        broadcaster_user_id: String,
        broadcaster_user_login: String,
        broadcaster_user_name: String,
        user_id: String,
        user_login: String,
        user_name: String,
        user_input: String,
        status: String,
        reward: Reward,
        redeemed_at: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct SubCondition {
        broadcaster_user_id: String,
        reward_id: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct SubscriptionTransport {
        method: String,
        callback: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Reward {
        id: String,
        title: String,
        cost: u16,
        prompt: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct VerificationEvent {
        challenge: String,
        subscription: SubInfo,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct SubInfo {
        id: String,
        status: String,
        r#type: String,
        version: String,
        cost: u32,
        condition: ConditionInfo,
        transport: TransportInfo,
        created_at: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct ConditionInfo {
        broadcaster_user_id: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct TransportInfo {
        method: String,
        callback: String,
    }

    impl VerificationEvent {
        pub fn challenge(&self) -> &str {
            &self.challenge
        }
    }
}
