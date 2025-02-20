use duration_str::deserialize_duration;
use std::{fmt::Display, path::PathBuf, time::Duration};

use serde::Deserialize;

/// GlobalParams holds the configuration parameters for the global settings of the application.
///
/// # Fields
///
/// - **sample_size** - Specifies the number of times each experiment is repeated.
/// - **output_directory** - The directory where benchmark results and outputs are stored. This variable can be referenced in task configurations using {{ output_directory }}.
/// - **extra_perf_events** - An optional vector of strings for additional performance monitoring events to be collected.
/// - **debug** - A boolean flag for enabling debug logging for more detailed output. Defaults to false.
/// - **deep_trace** - A boolean flag for enabling an extra experiment with tracing enabled. This can be very slow. Defaults to false.
/// - **energy_sample_interval** - The interval for energy sampling, deserialized using deserialize_duration. Must be specified with a time unit (e.g., "250ms" for 250 milliseconds). Defaults to 500 milliseconds.
#[derive(Deserialize, Debug)]
pub struct GlobalParams {
  pub sample_size: u32,
  pub output_directory: PathBuf,
  pub extra_perf_events: Option<Vec<String>>,

  #[serde(default)]
  pub debug: bool,

  #[serde(default)]
  pub deep_trace: bool,

  #[serde(
    deserialize_with = "deserialize_duration",
    default = "default_energy_sample_interval"
  )]
  pub energy_sample_interval: Duration,
}

/// Task represents a task to be executed, including its configuration and associated scripts.
///
/// # Fields
///
/// * **executable** - The path to the executable file for the task.
/// * **args** - A vector of arguments to be passed to the executable. Defaults to an empty vector.
/// - **num_threads** - A vector specifying the number of threads to be used in each experiment.
/// - **enclave_size** - A vector of strings representing the possible enclave memory sizes. Each experiment will be run with every listed size.
/// * **custom_manifest_path** - An optional path to a custom manifest file.
/// * **storage_type** - A vector of storage types, deserialized using **deserialize_storage_type**. Defaults to **[StorageType::Untrusted]**.
/// * **pre_run_executable** - An optional path to an executable to run before the main task.
/// * **pre_run_args** - A vector of arguments for the pre-run executable. Defaults to an empty vector.
/// * **post_run_executable** - An optional path to an executable to run after the main task.
/// * **post_run_args** - A vector of arguments for the post-run executable. Defaults to an empty vector.
#[derive(Deserialize, Clone, Debug)]
pub struct Task {
  pub executable: PathBuf,

  #[serde(default)]
  pub args: Vec<String>,

  pub num_threads: Option<Vec<usize>>,
  pub enclave_size: Vec<String>,

  pub custom_manifest_path: Option<PathBuf>,
  #[serde(
    default = "default_storage_type",
    deserialize_with = "deserialize_storage_type"
  )]
  pub storage_type: Vec<StorageType>,

  pub pre_run_executable: Option<PathBuf>,
  #[serde(default)]
  pub pre_run_args: Vec<String>,

  pub post_run_executable: Option<PathBuf>,
  #[serde(default)]
  pub post_run_args: Vec<String>,
}

/// StorageType defines the types of storage that can be used.
///
/// # Variants
///
/// - **Encrypted** - Represents encrypted storage.
/// - **Tmpfs** - Represents temporary file system storage.
/// - **Untrusted** - Represents untrusted storage.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
  Encrypted,
  Tmpfs,
  Untrusted,
}

impl Display for StorageType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Encrypted => write!(f, "encrypted"),
      Self::Tmpfs => write!(f, "tmpfs"),
      Self::Untrusted => write!(f, "untrusted"),
    }
  }
}
// ensure storage type is not empty
// could happen if the user writes storage_type = []
fn deserialize_storage_type<'de, D>(deserializer: D) -> Result<Vec<StorageType>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  let v: Option<Vec<StorageType>> = Option::deserialize(deserializer)?;
  Ok(if let Some(array) = v {
    if array.is_empty() {
      default_storage_type()
    } else {
      array
    }
  } else {
    default_storage_type()
  })
}

pub fn default_energy_sample_interval() -> Duration {
  Duration::from_millis(500)
}
pub fn default_storage_type() -> Vec<StorageType> {
  vec![StorageType::Untrusted]
}
