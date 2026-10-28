use std::{
    collections::HashMap,
    ops::{Add, AddAssign},
};

use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::{
    Mod, Templates,
    logs::{LogFields, Record},
};

type WeightMatrix = Vec<Vec<f64>>;

/// A matrix of frequency of elements at each position in a Scenario.
/// i.e `matrix[scenario_position][elements_index]`
#[derive(Clone, Debug)]
pub(crate) struct CountMatrix {
    matrix: Vec<Vec<u64>>,
}

impl CountMatrix {
    fn new(matrix: Vec<Vec<u64>>) -> Self {
        Self { matrix }
    }

    fn from(log: &LogFields, elements: &[Mod]) -> Self {
        Self {
            matrix: log
                .picks
                .iter()
                .map(|p| {
                    let mut counts = vec![0; elements.len()];
                    if let Some(pos) = index(elements, p) {
                        counts[pos] += 1
                    };

                    counts
                })
                .collect(),
        }
    }

    fn matrix(&self) -> &[Vec<u64>] {
        &self.matrix
    }
}

impl Add for CountMatrix {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            matrix: self
                .matrix
                .iter()
                .zip(rhs.matrix)
                .map(|(lhs_row, rhs_row)| {
                    lhs_row
                        .iter()
                        .zip(rhs_row)
                        .map(|(a, b)| a.saturating_add(b))
                        .collect()
                })
                .collect(),
        }
    }
}

impl AddAssign<&CountMatrix> for CountMatrix {
    fn add_assign(&mut self, rhs: &CountMatrix) {
        for (lhs_row, rhs_row) in self.matrix.iter_mut().zip(rhs.matrix()) {
            for (total, count) in lhs_row.iter_mut().zip(rhs_row) {
                *total = total.saturating_add(*count)
            }
        }
    }
}

fn index(elements: &[Mod], name: &str) -> Option<usize> {
    elements.iter().position(|x| x.name == name)
}

impl Templates {
    fn update(
        &mut self,
        template: &str,
        weights: &WeightMatrix,
        history: &CountMatrix,
    ) -> Result<()> {
        for scenario in self.scenarios.iter_mut() {
            if scenario.template == template {
                scenario.correction_weights = Some(weights.to_vec());
                scenario.history = Some(history.matrix().to_vec());
                scenario.latest_correction_at = Some(Utc::now().timestamp_millis());
                return Ok(());
            }
        }
        Err(anyhow!("No scenario found for this template: {template}"))
    }
}

pub(crate) struct WeightCorrections {
    templates: Templates,
    historical_counts: HashMap<String, CountMatrix>,
    latest_correction_at: Option<i64>,
}

impl WeightCorrections {
    fn from(templates: &Templates) -> Self {
        let historical: HashMap<String, CountMatrix> = templates
            .scenarios
            .iter()
            .map(|s| {
                let amount = s.winners.len() + s.others.len();
                (
                    s.template.clone(),
                    CountMatrix::new(
                        s.history
                            .clone()
                            .unwrap_or_else(|| vec![vec![0; templates.mods.len()]; amount]),
                    ),
                )
            })
            .collect();

        let latest_ts: Option<i64> = templates
            .scenarios
            .iter()
            .filter_map(|s| s.latest_correction_at)
            .max();

        Self {
            templates: templates.clone(),
            historical_counts: historical,
            latest_correction_at: latest_ts,
        }
    }

    fn update_counts(&mut self, records: &[Record]) -> HashMap<String, CountMatrix> {
        let mut observed: HashMap<String, CountMatrix> = HashMap::new();
        for r in records
            .iter()
            .filter(|r| r.timestamp > self.latest_correction_at)
        {
            let counts = CountMatrix::from(&r.fields, &self.templates.mods);
            observed
                .entry(r.fields.template.clone())
                .and_modify(|h| *h += &counts)
                .or_insert(counts);
        }

        for (template, counts) in observed.iter() {
            self.historical_counts
                .entry(template.to_string())
                .and_modify(|h| *h += counts)
                .or_insert(counts.clone());
        }

        observed
    }

    fn finish(&self) -> Templates {
        self.templates.clone()
    }

    fn num_candidates(&self) -> usize {
        self.templates.mods.len()
    }
}

/// Calculate a correction weight for each element at a certain position in a Scenario.
/// `W_j_i = (counts[j][i] + alpha).pow(-lambda)`
/// where:
/// alpha > 0 is a smoothing constant,
/// 0.0 <= lambda <= 1.0 is the correction strength,
/// and max_ratio >= 1.0 helps clamp the weights
pub(crate) fn calculate_correction(
    counts: &CountMatrix,
    size: usize,
    alpha: f64,
    lambda: f64,
    max_ratio: f64,
) -> WeightMatrix {
    assert!(alpha.is_finite() && alpha > 0.0);
    assert!(lambda.is_finite() && lambda >= 0.0);
    assert!(max_ratio >= 1.0);
    assert!(
        counts
            .matrix()
            .iter()
            .all(|candidates| candidates.len() == size)
    );

    counts
        .matrix()
        .iter()
        .map(|row| {
            let weights: Vec<f64> = row
                .iter()
                .map(|&count| ((count as f64).add(alpha)).powf(-lambda))
                .collect();

            let weight_ceil = weights
                .iter()
                .min_by(|a, b| a.total_cmp(b))
                .map_or(f64::INFINITY, |&min| min * max_ratio);

            weights.into_iter().map(|w| w.min(weight_ceil)).collect()
        })
        .collect()
}

pub(crate) fn compute_correction_weights(
    records: &[Record],
    config: &Templates,
) -> Result<Templates> {
    let mut weights = WeightCorrections::from(config);
    let observed = weights.update_counts(records);

    let size = weights.num_candidates();
    let alpha = 5.0;
    let lambda = 0.5;
    let max_ratio = 3.0;
    for template in observed.keys() {
        let counts = match weights.historical_counts.get(template) {
            Some(v) => v,
            None => return Err(anyhow!("Historical counts for this scenario uninitialized")),
        };
        let updated = calculate_correction(counts, size, alpha, lambda, max_ratio);
        weights.templates.update(template, &updated, counts)?;
    }
    Ok(weights.finish())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::correct::compute_correction_weights;
    use crate::logs::{LogFields, Record};
    use crate::{Mod, Scenario, Templates};

    #[test]
    fn compute_correction_weights_from_logs() -> Result<()> {
        let old_ts: i64 = 1782457959000;
        let new_ts: i64 = 1792998811000;

        let templates = Templates {
            scenarios: vec![Scenario {
                template: "Template 1".to_owned(),
                winners: vec!["3".to_owned()],
                others: vec!["2".to_owned(), "1".to_owned()],
                history: None,
                correction_weights: None,
                latest_correction_at: Some(old_ts),
            }],
            mods: vec![
                Mod {
                    name: "John".to_owned(),
                    weight: 0.25,
                },
                Mod {
                    name: "Jane".to_owned(),
                    weight: 0.25,
                },
                Mod {
                    name: "Alice".to_owned(),
                    weight: 0.25,
                },
                Mod {
                    name: "Bob".to_owned(),
                    weight: 0.25,
                },
            ],
        };
        let records: Vec<Record> = vec![
            log_entry("Template 1", &["John", "Jane", "Bob"], new_ts),
            log_entry("Template 1", &["John", "Jane", "Alice"], new_ts),
            log_entry("Template 1", &["Alice", "Jane", "Bob"], new_ts),
            log_entry("Template 1", &["John", "Bob", "Jane"], new_ts),
            log_entry("Template 1", &["John", "Bob", "Jane"], new_ts),
            log_entry("Template 1", &["Alice", "Bob", "Jane"], new_ts),
            log_entry("Template 1", &["Bob", "Jane", "Alice"], new_ts),
            log_entry("Template 1", &["Bob", "Jane", "John"], new_ts),
        ];

        let result = compute_correction_weights(&records, &templates);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn compute_correction_weights_only_for_new_occurrences() -> Result<()> {
        let old_ts: i64 = 1782457959000;
        let new_ts: i64 = 1792998811000;

        let templates = Templates {
            scenarios: vec![
                Scenario {
                    template: "Template 1".to_owned(),
                    winners: vec!["3".to_owned()],
                    others: vec!["2".to_owned(), "1".to_owned()],
                    history: None,
                    correction_weights: None,
                    latest_correction_at: Some(old_ts),
                },
                Scenario {
                    template: "Template 2".to_owned(),
                    winners: vec!["1".to_owned()],
                    others: vec!["2".to_owned()],
                    history: None,
                    correction_weights: None,
                    latest_correction_at: Some(old_ts),
                },
            ],
            mods: vec![
                Mod {
                    name: "John".to_owned(),
                    weight: 0.5,
                },
                Mod {
                    name: "Jane".to_owned(),
                    weight: 0.5,
                },
            ],
        };
        let records: Vec<Record> = vec![
            log_entry("Template 1", &["John", "Jane"], old_ts),
            log_entry("Template 2", &["Jane"], old_ts),
            log_entry("Template 1", &["Jane", "John"], old_ts),
            log_entry("Template 2", &["Jane"], new_ts),
        ];

        let result = compute_correction_weights(&records, &templates);
        assert!(result.is_ok());

        let updated = result.unwrap();
        assert!(updated.scenarios[0].latest_correction_at < Some(new_ts));
        assert!(updated.scenarios[1].latest_correction_at > Some(old_ts));
        Ok(())
    }

    fn log_entry(template: &str, picks: &[&str], ts: i64) -> Record {
        let fields = LogFields {
            template: template.to_owned(),
            picks: picks.iter().map(|s| s.to_string()).collect(),
        };

        Record {
            fields,
            timestamp: Some(ts),
        }
    }
}
