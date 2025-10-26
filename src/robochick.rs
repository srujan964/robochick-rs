pub mod twitch {
    use std::{collections::HashMap, error, fmt, iter::zip, vec};

    use fastrand::Rng;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct MessageComponents {
        pub(crate) scenarios: Vec<Scenario>,
        pub(crate) mods: Vec<String>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Scenario {
        pub(crate) template: String,
        pub(crate) winners: Vec<String>,
        pub(crate) others: Vec<String>,
    }

    #[derive(Debug)]
    pub enum ScenarioError {
        NotEnoughPlaceholders(String),
        InvalidValue(String),
        PickFailed(String),
    }

    impl fmt::Display for ScenarioError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                ScenarioError::InvalidValue(s) => write!(f, "InvalidValue({s})"),
                ScenarioError::NotEnoughPlaceholders(s) => write!(f, "NotEnoughPlaceholders({s})"),
                ScenarioError::PickFailed(s) => write!(f, "PickFailed({s})"),
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

    pub trait MessageBuilder {
        fn build_from_templates(
            message_components: &MessageComponents,
            rng: &mut Rng,
        ) -> Result<String, ScenarioError>;
    }

    pub struct Robochick {}

    impl Robochick {
        pub fn new() -> Robochick {
            Robochick {}
        }
    }

    impl MessageBuilder for Robochick {
        fn build_from_templates(
            message_components: &MessageComponents,
            rng: &mut Rng,
        ) -> Result<String, ScenarioError> {
            let mods: &[String] = message_components.get_mods();
            let scenarios: &[Scenario] = message_components.get_scenarios();

            if let Some(scenario_pick) = pick_random(scenarios, 1, rng).pop() {
                let m = scenario_pick.get_winners().len();
                let n = scenario_pick.get_others().len();

                let picks = pick_random(mods, m + n, rng);

                // Calling `pick_random()` once for each `m` and `n` had an edge case where
                // it picked the same mod into both vecs.
                // So this makes sure they're mutually exclusive.
                let (winners, others) = match picks.split_at_checked(m) {
                    Some((x, y)) => (x, y),
                    None => {
                        return Err(ScenarioError::PickFailed(
                            "Failed to pick {m + n} mods".into(),
                        ));
                    }
                };

                scenario_pick.build(&winners, &others)
            } else {
                Err(ScenarioError::PickFailed(
                    "Failed to select a scenario".into(),
                ))
            }
        }
    }

    fn pick_random<T: Clone>(haystack: &[T], amount: usize, rng: &mut Rng) -> Vec<T> {
        if haystack.is_empty() || amount == 0 {
            return vec![];
        }

        if amount == 1 {
            return match rng.choice(haystack) {
                Some(pick) => vec![pick.clone()],
                None => vec![],
            };
        }

        rng.choose_multiple(haystack, amount)
            .iter()
            .cloned()
            .cloned()
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use anyhow::Result;
        use fastrand::Rng;

        use crate::robochick::twitch::{
            MessageBuilder, MessageComponents, Robochick, Scenario, pick_random,
        };

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
        fn pick_random_returns_empty_vec_if_picking_any_amount_from_an_empty_list() -> Result<()> {
            let mut rng = Rng::with_seed(1_000);
            let result = pick_random::<String>(&vec![], 1, &mut rng);

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

        #[test]
        fn build_from_templates_should_return_a_built_scenario_message() -> Result<()> {
            let scenarios: Vec<Scenario> = vec![Scenario {
                template: "{placeholder} wins by default.".into(),
                winners: vec!["placeholder".into()],
                others: vec![],
            }];
            let mods: Vec<String> = vec!["John".into()];
            let message_components = MessageComponents { scenarios, mods };
            let mut rng = Rng::with_seed(1);

            let msg = Robochick::build_from_templates(&message_components, &mut rng)?;

            assert_eq!(msg, "John wins by default.");
            Ok(())
        }

        #[test]
        fn build_from_templates_should_return_err_if_message_components_has_no_scenarios()
        -> Result<()> {
            let mods: Vec<String> = vec!["John".into()];
            let message_components = MessageComponents {
                scenarios: vec![],
                mods,
            };
            let mut rng = Rng::with_seed(1);

            let result = Robochick::build_from_templates(&message_components, &mut rng);
            assert!(result.is_err());

            Ok(())
        }

        #[test]
        fn build_from_templates_should_succeed_if_no_placeholders_provided_in_template()
        -> Result<()> {
            let scenario = Scenario {
                template: "This sentence has no placeholders as intended.".into(),
                winners: vec![],
                others: vec![],
            };
            let mods: Vec<String> = vec!["Alice".into(), "Bob".into()];
            let message_components = MessageComponents {
                scenarios: vec![scenario],
                mods,
            };
            let mut rng = Rng::with_seed(1);

            let result = Robochick::build_from_templates(&message_components, &mut rng)?;
            assert_eq!("This sentence has no placeholders as intended.", result);

            Ok(())
        }

        #[test]
        fn build_from_templates_should_ensure_mod_chosen_as_winner_is_not_chosen_as_other()
        -> Result<()> {
            let scenario = Scenario {
                template: "{winner} is the winner, and a different person {other} is the loser."
                    .into(),
                winners: vec!["winner".into()],
                others: vec!["other".into()],
            };
            let mods: Vec<String> = vec!["John".into(), "Jane".into()];
            let message_components = MessageComponents {
                scenarios: vec![scenario],
                mods,
            };
            let mut rng: Rng = Rng::with_seed(1_000);

            let result = Robochick::build_from_templates(&message_components, &mut rng)?;

            // Expected message for this specific seed `1_000`
            assert_eq!(
                "John is the winner, and a different person Jane is the loser.",
                result
            );
            Ok(())
        }
    }
}
