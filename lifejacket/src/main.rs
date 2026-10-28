use std::io::Write;
use std::{path::PathBuf, time::Duration};

use crate::correct::compute_correction_weights;
use crate::logs::{DateRange, Query, parse};
use anyhow::{Context, Result, anyhow};
use aws_config::BehaviorVersion;
use aws_sdk_cloudwatchlogs::{Client, types::ResultField};
use chrono::{DateTime, NaiveDate, Utc};
use clap::Parser;
use serde::{Deserialize, Serialize};

mod correct;
mod logs;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Opt {
    /// Sets a log stream to read
    #[arg(short, long)]
    group: String,

    /// Sets a config file
    #[arg(short, long, value_name = "FILE")]
    config_path: PathBuf,

    /// Sets the query start date in YYYY-MM-DD format
    #[arg(short, long)]
    since: Option<String>,

    /// Sets the duration in seconds to wait for after starting a query
    #[arg(short, long)]
    wait: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Templates {
    pub(crate) scenarios: Vec<Scenario>,
    pub(crate) mods: Vec<Mod>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Mod {
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
    pub(crate) latest_correction_at: Option<i64>,
}

async fn fetch_logs(
    client: aws_sdk_cloudwatchlogs::Client,
    group: &str,
    date_range: DateRange,
    wait_s: u64,
) -> Result<Vec<Vec<ResultField>>> {
    let mut query = Query::new(
        client,
        group,
        date_range,
        "filter fields.message = \"Picks made\" | fields @timestamp, @message | sort @timestamp desc"
            .into(),
        Duration::from_secs(wait_s)
    );

    query.run().await.context("Error fetching logs")?;
    Ok(query.results)
}

fn read_config(path: &PathBuf) -> Result<Templates> {
    let config_str = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => {
            return Err(anyhow!(
                "Failed to read configuration file for building messages"
            ));
        }
    };

    serde_json::from_str::<Templates>(config_str.as_ref())
        .map_err(|e| anyhow!("Failed to deserialize message config: {e}"))
}

fn write_config(path: &PathBuf, contents: Templates) -> Result<()> {
    let mut writer = std::fs::File::options()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    serde_json::to_writer(&writer, &contents)?;
    writer.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let Opt {
        group,
        since,
        wait,
        config_path,
    } = Opt::parse();

    println!("Starting...");
    let config: Templates = read_config(&config_path)?;

    let now: DateTime<Utc> = Utc::now();
    let since_dt: DateTime<Utc> = since
        .map(|d| {
            NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .map(|dt| dt.and_time(chrono::NaiveTime::MIN).and_utc())
        })
        .transpose()?
        .unwrap_or_else(|| now - chrono::Duration::weeks(1));
    let wait_s = wait.unwrap_or(10);

    let aws_config = aws_config::from_env()
        .behavior_version(BehaviorVersion::latest())
        .load()
        .await;
    let client = Client::new(&aws_config);

    let logs = fetch_logs(client, &group, DateRange(since_dt, now), wait_s).await?;
    if logs.is_empty() {
        eprintln!("No logs found. Exiting...");
        return Err(anyhow!("Logs needed for calculating correction weights"));
    }

    println!("Fetched {} logs", logs.len());
    let records = parse(&logs);

    let updated_templates: Templates = compute_correction_weights(&records, &config)?;
    write_config(&config_path, updated_templates.clone())?;

    println!("Calculated correction weights and wrote to file!");
    Ok(())
}
