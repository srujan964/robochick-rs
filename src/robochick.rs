pub mod twitch {
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

        use crate::robochick::twitch::pick_random;

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
    }
}
