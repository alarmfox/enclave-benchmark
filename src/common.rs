use duration_str::deserialize_duration;
use std::{fmt::Display, path::PathBuf, time::Duration};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct GlobalParams {
    pub sample_size: u32,
    pub num_threads: Vec<usize>,
    pub enclave_size: Vec<String>,
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
pub fn default_energy_sample_interval() -> Duration {
    Duration::from_millis(500)
}

#[derive(Deserialize, Clone, Debug)]
pub struct Task {
    pub executable: PathBuf,
    pub args: Option<Vec<String>>,
    pub custom_manifest_path: Option<PathBuf>,
    #[serde(
        default = "default_storage_type",
        deserialize_with = "deserialize_storage_type"
    )]
    pub storage_type: Vec<StorageType>,

    pub pre_run_executable: Option<PathBuf>,
    pub pre_run_args: Option<Vec<String>>,

    pub post_run_executable: Option<PathBuf>,
    pub post_run_args: Option<Vec<String>>,
}

pub fn default_storage_type() -> Vec<StorageType> {
    vec![StorageType::Untrusted]
}

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
