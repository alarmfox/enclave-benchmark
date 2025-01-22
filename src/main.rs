use serde::Deserialize;
use std::{
    fs::{create_dir, create_dir_all, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use clap::{arg, command, Parser};
use tracing::Level;

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
    sample_size: u32,
    num_threads: Vec<usize>,
    epc_cache_size: Vec<usize>,
    tasks: Vec<Task>,
    output_directory: PathBuf,
}

#[derive(Deserialize, Clone, Debug)]
struct Task {
    executable: PathBuf,
    args: Option<Vec<String>>,
    manifest_path: PathBuf,
}

#[derive(Debug)]
struct Profiler {
    sample_size: u32,
    output_directory: PathBuf,
    experiments: Vec<Experiment>,
}

#[derive(Debug)]
struct Experiment {
    threads: usize,
    epc_cache_size: usize,
}

impl Profiler {
    fn new(
        sample_size: u32,
        num_threads: Vec<usize>,
        epc_cache_size: Vec<usize>,
        output_directory: PathBuf,
    ) -> Result<Self, std::io::Error> {
        let mut experiments: Vec<Experiment> = vec![];

        for &threads in &num_threads {
            for &cache in &epc_cache_size {
                experiments.push(Experiment {
                    threads,
                    epc_cache_size: cache,
                });
            }
        }

        create_dir_all(&output_directory)?;

        Ok(Profiler {
            sample_size,
            experiments,
            output_directory,
        })
    }

    #[tracing::instrument(level = "info", ret)]
    fn profile(self: &Self, task: Task) {
        // enclave only for now
    }

    #[tracing::instrument(level = "trace", ret)]
    fn run_outside_enclave(self: &Self, t: Task) -> Result<(), Box<dyn std::error::Error>> {
        let args = t.args.unwrap_or_default();
        let mut process = Command::new("sh")
            .arg("-c")
            .arg(t.executable)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        match process.wait() {
            Ok(exit_status) => match exit_status.success() {
                true => Ok(()),
                false => panic!("program exited with non zero code"),
            },
            Err(e) => panic!("program crashed {e}"),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    let log_level = match cli.verbose {
        0 => Level::ERROR,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };
    let mut config = String::new();
    tracing_subscriber::fmt().with_max_level(log_level).init();
    let _ = File::open(cli.config_path)?.read_to_string(&mut config)?;
    let config = toml::from_str::<Config>(config.as_str())?;
    let profiler = Profiler::new(
        config.sample_size,
        config.num_threads,
        config.epc_cache_size,
        config.output_directory,
    )?;

    for task in config.tasks {
        profiler.run_outside_enclave(task)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::fs::remove_dir_all;

    use crate::*;
    #[test]
    fn parse_config() {
        let config = toml::from_str::<Config>(
            r#"
            sample_size = 3
            epc_cache_size = [64, 128]
            num_threads = [1]
            output_directory = "/test"
            [[tasks]]
            executable = "/bin/ls"
            manifest_path = "/bin/ls"
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            manifest_path = "/bin/ls"
            "#,
        )
        .unwrap();
        let config = dbg!(config);
        assert_eq!(2, config.tasks.len());
        assert_eq!(3, config.sample_size);
        let args = config.tasks[1].clone().args.unwrap();
        assert_eq!(2, args.len());
        assert_eq!(1, config.num_threads.len());
        assert_eq!(2, config.epc_cache_size.len());
        assert_eq!(1, config.num_threads[0]);
    }

    #[test]
    fn simple_profile() {
        let config = toml::from_str::<Config>(
            r#"
            sample_size = 3
            epc_cache_size = [64, 128]
            num_threads = [1]
            output_directory = "/tmp/test"
            [[tasks]]
            executable = "/bin/ls"
            manifest_path = "/bin/ls"
            "#,
        )
        .unwrap();
        Profiler::new(
            config.sample_size,
            config.num_threads,
            config.epc_cache_size,
            config.output_directory.clone(),
        )
        .unwrap()
        .run_outside_enclave(config.tasks[0].clone())
        .unwrap();

        remove_dir_all(config.output_directory).unwrap();
    }
}
