pub mod twitch {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    pub struct MessageComponents {
        pub(crate) scenarios: Vec<Scenario>,
        pub(crate) mods: Vec<String>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Scenario {
        pub(crate) template: String,
        pub(crate) winners: Vec<String>,
        pub(crate) others: Vec<String>,
    }

    impl Scenario {
        pub fn get_template(&self) -> &str {
            &self.template
        }

        pub fn get_winners(&self) -> &[String] {
            &self.winners
        }

        pub fn get_others(&self) -> &[String] {
            &self.others
        }
    }

    impl MessageComponents {
        pub fn get_mods(&self) -> &[String] {
            &self.mods
        }

        pub fn get_scenarios(&self) -> &[Scenario] {
            &self.scenarios
        }
    }
}
