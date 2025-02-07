use collector::DefaultCollector;
use common::{GlobalParams, Task};
use profiler::Profiler;
use serde::Deserialize;
use std::{
    fmt::Debug,
    fs::{remove_dir_all, File},
    io::Read,
    path::PathBuf,
};

use clap::{arg, command, Parser};
use tracing::{warn, Level};

mod collector;
mod common;
mod constants;
mod profiler;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Turn debugging information on
    #[arg(short, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long, default_value = "workload.toml")]
    config_path: PathBuf,

    #[arg(long, default_value = "false")]
    force: bool,
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

    if cli.force {
        warn!("force specified; deleting previous results directory...");
        match remove_dir_all(config.globals.output_directory.clone()) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            v => v?,
        }
    }

    let profiler = Profiler::new(
        config.globals.num_threads,
        config.globals.enclave_size,
        config.globals.output_directory,
        config.globals.debug,
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

#[cfg(test)]
mod test {
    use crate::Config;

    #[test]
    fn parse_config() {
        let config = toml::from_str::<Config>(
            r#"
            [globals]
            sample_size = 3
            enclave_size = ["64M", "128M"]
            num_threads = [1]
            output_directory = "/test"
            debug = true
            [[tasks]]
            executable = "/bin/ls"
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            storage_type = ["encrypted", "tmpfs"] 
            "#,
        )
        .unwrap();
        assert!(config.globals.debug);
        assert_eq!(2, config.tasks.len());
        assert_eq!(3, config.globals.sample_size);
        let args = config.tasks[1].clone().args.unwrap();
        assert_eq!(2, args.len());
        assert_eq!(1, config.globals.num_threads.len());
        assert_eq!(2, config.globals.enclave_size.len());
        assert_eq!(1, config.globals.num_threads[0]);
    }
}
