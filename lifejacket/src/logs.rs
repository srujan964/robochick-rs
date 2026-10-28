use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use aws_sdk_cloudwatchlogs::types::ResultField;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Deserializer, de::DeserializeOwned};

#[derive(Clone, Debug)]
pub(crate) struct DateRange(pub(crate) DateTime<Utc>, pub(crate) DateTime<Utc>);

pub(crate) struct Query {
    pub(crate) client: aws_sdk_cloudwatchlogs::Client,
    pub(crate) log_group_name: String,
    pub(crate) date_range: DateRange,
    pub(crate) filter_message: String,
    pub(crate) wait: Duration,
    pub(crate) results: Vec<Vec<ResultField>>,
}

impl Query {
    pub(crate) fn new(
        client: aws_sdk_cloudwatchlogs::Client,
        group: &str,
        date_range: DateRange,
        filter: String,
        wait_s: Duration,
    ) -> Self {
        Self {
            client,
            log_group_name: group.to_owned(),
            date_range,
            filter_message: filter,
            wait: wait_s,
            results: vec![],
        }
    }

    // Start a log query and wait for its result.
    pub(crate) async fn run(&mut self) -> Result<()> {
        let query = self
            .client
            .start_query()
            .query_string(&self.filter_message)
            .start_time(self.date_range.0.timestamp())
            .end_time(self.date_range.1.timestamp())
            .limit(500)
            .log_group_name(&self.log_group_name)
            .send()
            .await
            .context("Error starting log query")?;

        let id = query.query_id().context("Query should have an id")?;

        println!("Started query, waiting for {:?}...", self.wait);
        tokio::time::sleep(self.wait).await;

        let resp = self
            .client
            .get_query_results()
            .query_id(id)
            .send()
            .await
            .map_err(|e| anyhow!("Error fetching query results: {}", e.into_service_error()))?;

        resp.results
            .map(|r| self.results = r)
            .ok_or_else(|| anyhow!("No logs returned by query"))
    }
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct Log {
    fields: LogFields,
}

#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub(crate) fields: LogFields,
    pub(crate) timestamp: Option<i64>,
}

fn deserialize_json_string<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let val = String::deserialize(deserializer)?;
    serde_json::from_str(&val).map_err(serde::de::Error::custom)
}

#[derive(Clone, Deserialize, Debug)]
pub(crate) struct LogFields {
    #[serde(alias = "templates")]
    pub(crate) template: String,
    #[serde(deserialize_with = "deserialize_json_string")]
    pub(crate) picks: Vec<String>,
}

pub(crate) fn parse(logs: &[Vec<ResultField>]) -> Vec<Record> {
    logs.iter()
        .map(|log| {
            let message = log
                .iter()
                .find(|&f| f.field().is_some_and(|field_name| field_name == "@message"))
                .and_then(|f| f.value())
                .map(|v| serde_json::from_str::<Log>(v).expect("Field should be valid json"))
                .expect("Log should have `message`");

            let ts = log
                .iter()
                .find(|&f| {
                    f.field()
                        .is_some_and(|field_name| field_name == "@timestamp")
                })
                .and_then(|f| f.value())
                .map(|v| {
                    NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M:%S%.3f")
                        .expect("Invalid timestamp")
                })
                .map(|ts| ts.and_utc().timestamp_millis());

            Record {
                fields: message.fields,
                timestamp: ts,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::logs::parse;
    use anyhow::Result;
    use aws_sdk_cloudwatchlogs::types::ResultField;

    #[test]
    fn parse_extracts_log_fields() -> Result<()> {
        let fields = vec![
            ResultField::builder()
                .field("@message")
                .value(r#"{"level":"INFO","fields":{"message":"Picks made","picks":"[\"first\"]","template":"{winner_1}"}}"#)
                .build(),
        ];

        let mut records = parse(&[fields]);
        assert_eq!(records.len(), 1);

        let record = records.pop().unwrap();
        assert_eq!(record.fields.picks, vec!["first".to_owned()]);
        assert_eq!(record.fields.template, "{winner_1}".to_owned());
        Ok(())
    }
}
