use collector::DefaultCollector;
use common::{GlobalParams, Task};
use profiler::Profiler;
use serde::Deserialize;
use std::{
  env,
  fmt::Debug,
  fs::{remove_dir_all, File},
  io::Read,
  path::PathBuf,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};

use clap::{arg, command, Parser};
use tracing::{info, warn, Level};

mod collector;
mod common;
mod constants;
mod profiler;
mod stats;

mod tracer {
  include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bpf/tracer.skel.rs"
  ));
}

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"), about = "A cli app to run benchmarks for Gramine application", long_about = None)]
#[command(name = "enclave-benchmark")]
struct Cli {
  /// Turn debugging information on
  #[arg(short, action = clap::ArgAction::Count)]
  verbose: u8,

  #[arg(short, long, help = "Path to configuration file")]
  config: PathBuf,

  #[arg(
    long,
    default_value = "false",
    help = "Remove previous results directory (if exists)"
  )]
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
  if env::var_os("EB_SKIP_SGX").is_some_and(|v| v == "1") {
    warn!("EB_SKIP_SGX is set; skipping SGX execution");
  }
  let mut config = String::new();
  tracing_subscriber::fmt().with_max_level(log_level).init();
  let n = File::open(cli.config)?.read_to_string(&mut config)?;
  let config = toml::from_str::<Config>(&config[..n])?;

  if cli.force {
    warn!("force specified; deleting previous results directory...");
    match remove_dir_all(config.globals.output_directory.clone()) {
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
      v => v?,
    }
  }

  let collector = Arc::new(DefaultCollector::new(
    config.globals.sample_size,
    config.globals.deep_trace,
    config.globals.energy_sample_interval,
    config.globals.extra_perf_events,
  ));

  let profiler = Profiler::new(
    config.globals.output_directory,
    config.globals.debug,
    collector.clone(),
  )?;

  let collector = collector.clone();
  let stop = Arc::new(AtomicBool::new(false));
  {
    let stop = stop.clone();
    let collector = collector.clone();
    ctrlc::set_handler(move || {
      info!("Received stop signal. Closing existing threads... ");
      collector.clone().stop();
      stop.store(true, Ordering::Relaxed);
    })
    .expect("Cannot set SIGTERM handler");
  }

  for task in config.tasks {
    if stop.clone().load(Ordering::Relaxed) {
      break;
    }
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
            output_directory = "/test"
            debug = true
            deep_trace = true
            [[tasks]]
            executable = "/bin/ls"
            enclave_size = ["64M", "128M"]
            num_threads = [1]
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            storage_type = ["encrypted", "tmpfs"] 
            enclave_size = ["64M", "128M"]
            num_threads = [1]
            "#,
    )
    .unwrap();
    assert!(config.globals.debug);
    assert_eq!(2, config.tasks.len());
    assert_eq!(3, config.globals.sample_size);
    let args = config.tasks[1].clone().args;
    assert_eq!(2, args.len());
  }
}
