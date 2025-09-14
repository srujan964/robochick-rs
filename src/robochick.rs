pub mod twitch {
    use std::{collections::HashMap, error, fmt, iter::zip};

    use anyhow::Result;
    use fastrand::Rng;
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

    #[derive(Debug)]
    pub enum ScenarioError {
        NotEnoughPlaceholders(String),
        InvalidValue(String),
    }

    impl fmt::Display for ScenarioError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                ScenarioError::InvalidValue(s) => write!(f, "InvalidValue({s})"),
                ScenarioError::NotEnoughPlaceholders(s) => write!(f, "NotEnoughPlaceholders({s})"),
            }
        }
    }

    impl error::Error for ScenarioError {}

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

        pub fn build(
            &self,
            winners: &[String],
            others: &[String],
        ) -> Result<String, ScenarioError> {
            if self.winners.len() != winners.len() {
                return Err(ScenarioError::NotEnoughPlaceholders(format!(
                    "Expected {} values, found {}",
                    self.winners.len(),
                    winners.len()
                )));
            }

            if self.others.len() != others.len() {
                return Err(ScenarioError::NotEnoughPlaceholders(format!(
                    "Expected {} values, found {}",
                    self.others.len(),
                    others.len()
                )));
            }

            let mut values: HashMap<String, String> = HashMap::new();
            for (k, v) in zip(self.winners.clone(), winners) {
                values.insert(k, v.to_string());
            }

            for (k, v) in zip(self.others.clone(), others) {
                values.insert(k, v.to_string());
            }

            match strfmt::strfmt(&self.template, &values) {
                Ok(msg) => Ok(msg),
                Err(e) => Err(ScenarioError::InvalidValue(format!(
                    "Failed to format string. Original error: {e}"
                ))),
            }
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

    pub fn pick_random(mods: &[String], n: usize, rng: &mut Rng) -> Vec<String> {
        if n == 0 {
            return vec![];
        }

        rng.choose_multiple(mods, n)
            .iter()
            .cloned()
            .cloned()
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use anyhow::Result;
        use fastrand::Rng;

        use crate::robochick::twitch::{Scenario, pick_random};

        #[test]
        fn pick_random_chooses_a_single_random_moderator() -> Result<()> {
            let mods: Vec<String> =
                vec!["John".into(), "Jane".into(), "Alex".into(), "Krish".into()];
            let mut rng = Rng::with_seed(1_000);

            let mut result = pick_random(&mods, 1, &mut rng);

            assert_eq!(result.len(), 1);
            let first_result = result.pop().unwrap();
            assert!(mods.iter().any(|e| *e == first_result));
            Ok(())
        }

        #[test]
        fn pick_random_chooses_multiple_mods() -> Result<()> {
            let mods: Vec<String> =
                vec!["John".into(), "Jane".into(), "Alex".into(), "Krish".into()];
            let mut rng = Rng::with_seed(1_000);

            let mut result = pick_random(&mods, 2, &mut rng);

            assert_eq!(result.len(), 2);
            let first_result = result.pop().unwrap();
            let second_result = result.pop().unwrap();
            assert!(mods.iter().any(|e| *e == first_result));
            assert!(mods.iter().any(|e| *e == second_result));
            Ok(())
        }

        #[test]
        fn pick_random_returns_empty_vec_if_n_is_zero() -> Result<()> {
            let mods: Vec<String> =
                vec!["John".into(), "Jane".into(), "Alex".into(), "Krish".into()];
            let mut rng = Rng::with_seed(1_000);

            let result = pick_random(&mods, 0, &mut rng);

            assert!(result.is_empty());
            Ok(())
        }

        #[test]
        fn scenario_build_returns_a_correctly_built_message() -> Result<()> {
            let scenario = Scenario {
                template: "{placeholder} is the expected {other_placeholder}".into(),
                winners: vec!["placeholder".into()],
                others: vec!["other_placeholder".into()],
            };

            let winners: Vec<String> = vec!["This".into()];
            let others: Vec<String> = vec!["sentence.".into()];

            let result = scenario.build(&winners, &others)?;

            assert_eq!("This is the expected sentence.", result);

            Ok(())
        }

        #[test]
        fn scenario_build_returns_err_on_incorrect_number_of_other_placeholders() -> Result<()> {
            let scenario = Scenario {
                template: "{placeholder} is the expected {other_placeholder}".into(),
                winners: vec!["placeholder".into()],
                others: vec!["other_placeholder".into(), "extra_placeholder".into()],
            };

            let winners: Vec<String> = vec!["This".into()];
            let others: Vec<String> = vec!["sentence.".into()];

            let result = scenario.build(&winners, &others);

            assert!(result.is_err());
            Ok(())
        }

        #[test]
        fn scenario_build_returns_err_on_incorrect_number_of_winner_placeholders() -> Result<()> {
            let scenario = Scenario {
                template: "{placeholder} is the expected {other_placeholder}".into(),
                winners: vec!["placeholder".into(), "extra_placeholder".into()],
                others: vec!["other_placeholder".into()],
            };

            let winners: Vec<String> = vec!["This".into()];
            let others: Vec<String> = vec!["sentence.".into()];

            let result = scenario.build(&winners, &others);

            assert!(result.is_err());
            Ok(())
        }
    }
}
