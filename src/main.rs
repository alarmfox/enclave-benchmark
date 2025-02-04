use collector::DefaultCollector;
use common::{GlobalParams, Task};
use profiler::Profiler;
use serde::Deserialize;
use std::{fmt::Debug, fs::File, io::Read, path::PathBuf};

use clap::{arg, command, Parser};
use tracing::Level;

mod collector;
mod common;
mod profiler;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Turn debugging information on
    #[arg(short, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long, default_value = "workload.toml")]
    config_path: PathBuf,
}

#[derive(Deserialize, Debug)]
struct Config {
    pub globals: GlobalParams,
    pub tasks: Vec<Task>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    let log_level = match cli.verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };
    let mut config = String::new();
    tracing_subscriber::fmt().with_max_level(log_level).init();
    let n = File::open(cli.config_path)?.read_to_string(&mut config)?;
    let config = toml::from_str::<Config>(&config[..n])?;

    let profiler = Profiler::new(
        config.globals.num_threads,
        config.globals.epc_size,
        config.globals.output_directory,
        DefaultCollector::new(
            config.globals.sample_size,
            config.globals.energy_sample_interval,
            config.globals.extra_perf_events,
        ),
    )?;

    for task in config.tasks {
        profiler.profile(task)?;
    }
    Ok(())
}

#[test]
fn test_parse_config() {
    let config = toml::from_str::<Config>(
        r#"
            [globals]
            sample_size = 3
            epc_size = ["64M", "128M"]
            num_threads = [1]
            output_directory = "/test"
            [[tasks]]
            executable = "/bin/ls"
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            storage_type = ["encrypted", "tmpfs", "trusted"]
            "#,
    )
    .unwrap();
    assert_eq!(2, config.tasks.len());
    assert_eq!(3, config.globals.sample_size);
    let args = config.tasks[1].clone().args.unwrap();
    assert_eq!(2, args.len());
    assert_eq!(1, config.globals.num_threads.len());
    assert_eq!(2, config.globals.epc_size.len());
    assert_eq!(1, config.globals.num_threads[0]);
}
