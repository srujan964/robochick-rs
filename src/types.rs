pub mod twitch {
    use std::fmt::Display;

    use serde::{Deserialize, Serialize};
    use strum::{AsRefStr, EnumString};

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

    #[derive(Debug, AsRefStr, EnumString)]
    pub enum MessageType {
        #[strum(serialize = "webhook_callback_verification")]
        WebhookCallbackVerification,
        #[strum(serialize = "notification")]
        Notification,
        #[strum(serialize = "revocation")]
        Revocation,
    }

    #[derive(Debug, AsRefStr, EnumString)]
    pub enum SubscriptionType {
        #[strum(serialize = "channel.channel_points_custom_reward_redemption.add")]
        CustomRewardRedemption,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct RewardRedeemed {
        pub(crate) subscription: Subscription,
        pub(crate) event: RewardEvent,
    }

    impl RewardRedeemed {
        pub fn broadcaster_user_id(&self) -> &str {
            &self.event.broadcaster_user_id
        }

        pub fn reward_id(&self) -> &str {
            &self.event.reward.id
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Subscription {
        id: String,
        r#type: String,
        version: String,
        status: String,
        cost: u16,
        condition: Condition,
        transport: Transport,
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
    pub struct Condition {
        broadcaster_user_id: String,
        reward_id: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Transport {
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
        subscription: Subscription,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct RevocationEvent {
        subscription: Subscription,
    }

    impl RevocationEvent {
        pub fn subscription_type(&self) -> &str {
            &self.subscription.r#type
        }

        pub fn subscription_status(&self) -> &str {
            &self.subscription.status
        }
    }

    impl VerificationEvent {
        pub fn challenge(&self) -> &str {
            &self.challenge
        }
    }
}
