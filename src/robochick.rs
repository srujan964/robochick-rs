pub mod twitch {
    use std::{collections::HashMap, error, fmt, iter::zip};

    use lambda_http::tracing;
    use rand::{
        Rng,
        distr::{Distribution, weighted::WeightedIndex},
        seq::IndexedRandom,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Templates {
        pub(crate) scenarios: Vec<Scenario>,
        pub(crate) mods: Vec<Mod>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Mod {
        pub(crate) name: String,
        pub(crate) weight: f64,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Scenario {
        pub(crate) template: String,
        pub(crate) winners: Vec<String>,
        pub(crate) others: Vec<String>,
        pub(crate) history: Option<Vec<Vec<u64>>>,
        pub(crate) correction_weights: Option<Vec<Vec<f64>>>,
    }

    #[derive(Debug)]
    pub enum ScenarioError {
        NotEnoughPlaceholders(String),
        InvalidValue(String),
        PickFailed(String),
        InvalidWeights,
    }

    impl fmt::Display for ScenarioError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                ScenarioError::InvalidValue(s) => write!(f, "InvalidValue({s})"),
                ScenarioError::NotEnoughPlaceholders(s) => write!(f, "NotEnoughPlaceholders({s})"),
                ScenarioError::PickFailed(s) => write!(f, "PickFailed({s})"),
                ScenarioError::InvalidWeights => write!(f, "InvalidWeights"),
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

        pub fn build(&self, winners: &[&str], others: &[&str]) -> Result<String, ScenarioError> {
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

    impl Templates {
        pub fn get_mods(&self) -> &[Mod] {
            &self.mods
        }

        pub fn get_scenarios(&self) -> &[Scenario] {
            &self.scenarios
        }

        pub fn default_weights(&self) -> Vec<f64> {
            self.mods.iter().map(|m| m.weight).collect()
        }
    }

    pub trait MessageBuilder {
        fn build_from_templates(
            templates: &Templates,
            rng: &mut impl rand::Rng,
        ) -> Result<String, ScenarioError>;
    }

    pub struct Robochick {}

    impl Robochick {
        pub fn new() -> Robochick {
            Robochick {}
        }
    }

    impl Default for Robochick {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MessageBuilder for Robochick {
        fn build_from_templates(
            templates: &Templates,
            rng: &mut impl rand::Rng,
        ) -> Result<String, ScenarioError> {
            let mods: &[Mod] = templates.get_mods();
            let scenarios: &[Scenario] = templates.get_scenarios();

            scenarios
                .choose(rng)
                .map(|scenario| populate(scenario, templates, rng))
                .unwrap_or(Err(ScenarioError::PickFailed(
                    "Failed to select a scenario".into(),
                )))
        }
    }

    fn populate(
        scenario: &Scenario,
        templates: &Templates,
        rng: &mut impl rand::Rng,
    ) -> Result<String, ScenarioError> {
        let m = scenario.winners.len();
        let n = scenario.others.len();
        let amount = m + n;
        let mut remaining: Vec<usize> = (0..amount).collect();
        let mut picks: Vec<&str> = Vec::with_capacity(amount);

        if let Some(weights) = &scenario.correction_weights {
            for weights_at_pos in weights.iter().take(amount) {
                let current_weights: Vec<f64> =
                    remaining.iter().map(|&i| weights_at_pos[i]).collect();
                let dist: WeightedIndex<f64> = WeightedIndex::new(&current_weights)
                    .map_err(|_| ScenarioError::InvalidWeights)?;

                let selection_idx = dist.sample(rng);
                let selection = &templates.mods[remaining.remove(selection_idx)];
                picks.push(&selection.name);
            }
        } else {
            picks.extend(templates.mods.sample(rng, amount).map(|m| m.name.as_str()));
        }
        tracing::info!(?picks, templates = %scenario.template, "Picks made");

        scenario.build(&picks[..m], &picks[m..])
    }

    #[cfg(test)]
    mod tests {
        use crate::robochick::twitch::{MessageBuilder, Mod, Robochick, Scenario, Templates};
        use anyhow::Result;
        use rand::{SeedableRng, rngs::SmallRng};

        fn populate_scenario_with_default_weights() -> Result<()> {
            let scenario = Scenario {
                template: "{winner} is the winner.".into(),
                winners: vec!["winner".into()],
                others: vec![],
                history: None,
                correction_weights: None,
            };
            let mods: Vec<Mod> = vec![
                Mod {
                    name: "John".into(),
                    weight: 1.0,
                },
                Mod {
                    name: "Jane".into(),
                    weight: 0.0,
                },
            ];
            let templates = Templates {
                scenarios: vec![scenario],
                mods,
            };
            let mut rng: SmallRng = rand::make_rng();
            let message = Robochick::build_from_templates(&templates, &mut rng)?;

            assert_eq!("John is the winner.".to_owned(), message);
            Ok(())
        }

        #[test]
        fn populate_scenario_with_custom_weights() -> Result<()> {
            let scenario = Scenario {
                template: "{one}, {two}, and {three} in the fruit basket.".into(),
                winners: vec!["one".into()],
                others: vec!["two".into(), "three".into()],
                history: None,
                correction_weights: Some(vec![
                    vec![0.0, 0.0, 10.0, 0.0], // always choose 3rd into position 0
                    vec![10.0, 0.0, 0.0, 0.0], // always choose 1st into position 1
                    vec![0.0, 10.0, 0.0, 0.0], // always choose 2nd into position 2
                    vec![0.0, 0.0, 0.0, 10.0], // always choose 4th into position 3
                ]),
            };
            let mods: Vec<Mod> = vec![
                Mod {
                    name: "Apple".into(),
                    weight: 0.25,
                },
                Mod {
                    name: "Banana".into(),
                    weight: 0.25,
                },
                Mod {
                    name: "Cherry".into(),
                    weight: 0.25,
                },
                Mod {
                    name: "Date".into(),
                    weight: 0.25,
                },
            ];
            let templates = Templates {
                scenarios: vec![scenario],
                mods,
            };
            let mut rng: SmallRng = rand::SeedableRng::seed_from_u64(1000);
            let message = Robochick::build_from_templates(&templates, &mut rng)?;

            assert_eq!(
                "Cherry, Apple, and Banana in the fruit basket.".to_owned(),
                message
            );
            Ok(())
        }

        #[test]
        fn scenario_build_returns_a_correctly_built_message() -> Result<()> {
            let scenario = Scenario {
                template: "{placeholder} is the expected {other_placeholder}".into(),
                winners: vec!["placeholder".into()],
                others: vec!["other_placeholder".into()],
                history: None,
                correction_weights: None,
            };

            let winners = vec!["This"];
            let others = vec!["sentence."];

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
                history: None,
                correction_weights: None,
            };

            let winners = vec!["This"];
            let others = vec!["sentence."];

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
                history: None,
                correction_weights: None,
            };

            let winners = vec!["This"];
            let others = vec!["sentence."];
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
                history: None,
                correction_weights: None,
            }];
            let mods = vec![Mod {
                name: "John".into(),
                weight: 1.0,
            }];
            let templates = Templates { scenarios, mods };
            let mut rng: SmallRng = rand::SeedableRng::seed_from_u64(1_000);
            let msg = Robochick::build_from_templates(&templates, &mut rng)?;

            assert_eq!(msg, "John wins by default.");
            Ok(())
        }

        #[test]
        fn build_from_templates_should_return_err_if_templates_has_no_scenarios() -> Result<()> {
            let mods: Vec<Mod> = vec![Mod {
                name: "John".into(),
                weight: 1.0,
            }];
            let templates = Templates {
                scenarios: vec![],
                mods,
            };
            let mut rng: SmallRng = rand::SeedableRng::seed_from_u64(1_000);
            let result = Robochick::build_from_templates(&templates, &mut rng);
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
                history: None,
                correction_weights: None,
            };
            let mods: Vec<Mod> = vec![
                Mod {
                    name: "Alice".into(),
                    weight: 0.5,
                },
                Mod {
                    name: "Bob".into(),
                    weight: 0.5,
                },
            ];
            let templates = Templates {
                scenarios: vec![scenario],
                mods,
            };
            let mut rng: SmallRng = rand::SeedableRng::seed_from_u64(1_000);
            let result = Robochick::build_from_templates(&templates, &mut rng)?;
            assert_eq!("This sentence has no placeholders as intended.", result);

            Ok(())
        }
    }
}
