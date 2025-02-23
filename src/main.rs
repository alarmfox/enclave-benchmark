use collector::DefaultCollector;
use common::{GlobalParams, Task};
use profiler::Profiler;
use pyo3::{
  ffi::c_str,
  types::{PyAnyMethods, PyModule},
  Py, PyAny, PyResult, Python,
};
use serde::Deserialize;
use std::{
  env,
  fmt::Debug,
  fs::{self, remove_dir_all},
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

  #[arg(
    long,
    default_value = "false",
    help = "Aggregate results from samples. Creates an <output_directory>/aggregated"
  )]
  aggregate: bool,
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
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::filter::EnvFilter::from_default_env()
        .add_directive("handlebars=error".parse()?)
        .add_directive(format!("{}={}", module_path!(), log_level).parse()?),
    )
    .init();

  if env::var_os("EB_SKIP_SGX").is_some_and(|v| v == "1") {
    warn!("EB_SKIP_SGX is set; skipping SGX execution");
  }
  let config = fs::read_to_string(&cli.config)?;
  let config = toml::from_str::<Config>(&config)?;
  let output_directory = config.globals.output_directory.clone();

  if cli.force {
    warn!("force specified; deleting previous results directory...");
    match remove_dir_all(&config.globals.output_directory) {
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

  let profiler = Arc::new(Profiler::new(
    config.globals.output_directory,
    config.globals.debug,
    collector.clone(),
  )?);

  let collector = collector.clone();
  let stop = Arc::new(AtomicBool::new(false));
  {
    let stop = stop.clone();
    let collector = collector.clone();
    let profiler = profiler.clone();
    ctrlc::set_handler(move || {
      info!("Received stop signal. Closing existing threads... ");
      profiler.stop();
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

  if cli.aggregate {
    Python::with_gil(|py| -> PyResult<()> {
      let aggregate_script = c_str!(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/dev/aggregate.py"
      )));

      let output_directory = output_directory.join("aggregated");
      info!(
        "aggregating results in {:?}. This may take some time...",
        output_directory
      );
      let aggregate_fn: Py<PyAny> =
        PyModule::from_code(py, aggregate_script, c_str!(""), c_str!(""))?
          .getattr("aggregate")?
          .into();

      aggregate_fn.call1(py, (cli.config, output_directory))?;

      Ok(())
    })
    .unwrap();
  }

  Ok(())
}

#[cfg(test)]
mod test {
  use std::fs;

  use crate::{common::StorageType, Config};

  #[test]
  fn example_configs() {
    let examples = [
      "examples/full.toml",
      "examples/simple.toml",
      "examples/iobound.toml",
      "examples/minimal.toml",
      "examples/demo.toml",
    ];
    for file in examples {
      let content = fs::read_to_string(file).unwrap();
      toml::from_str::<Config>(&content).unwrap();
    }
  }

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
            storage_type = ["encrypted"] 
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

  #[test]
  fn default_storage_type() {
    let config = toml::from_str::<Config>(
      r#"
            [globals]
            sample_size = 3
            output_directory = "/test"
            [[tasks]]
            executable = "/bin/ls"
            storage_type = []
            enclave_size = ["64M", "128M"]
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            storage_type = ["encrypted"] 
            enclave_size = ["64M", "128M"]
            "#,
    )
    .unwrap();

    assert_eq!(config.tasks.len(), 2);
    assert_eq!(config.tasks[0].storage_type.len(), 1);
    assert_eq!(config.tasks[0].storage_type[0], StorageType::Untrusted);
  }

  #[test]
  #[should_panic]
  fn invalid_storage_type() {
    toml::from_str::<Config>(
      r#"
            [globals]
            sample_size = 3
            output_directory = "/test"
            [[tasks]]
            executable = "/bin/ls"
            enclave_size = ["64M", "128M"]
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            storage_type = ["invalid_storage_type", "encrypted"]
            enclave_size = ["64M", "128M"]
            "#,
    )
    .unwrap();
  }
}
