use std::{path::PathBuf, time::Instant};

use anyhow::{Context, Result, anyhow};

use rand::{Rng, rngs::SmallRng};
use robochick_rs::robochick::twitch::{MessageBuilder, Robochick, Templates};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: robochick_rs <FILE> <COUNT>");
        return Err(anyhow!("Incorrect usage"));
    }
    let filepath = PathBuf::from(&args[1]);
    let count: u32 = str::parse(&args[2]).context("Arg 2 must be an integer")?;

    let contents = std::fs::read_to_string(filepath)?;
    let templates: Templates = serde_json::from_str(&contents)?;
    let mut rng: SmallRng = rand::make_rng();

    for i in 0..count {
        let start = Instant::now();
        let message = Robochick::build_from_templates(&templates, &mut rng)?;
        let elapsed = start.elapsed();
        println!("Message built in {:?}:\n - {}\n", elapsed, message);
    }
    Ok(())
}
