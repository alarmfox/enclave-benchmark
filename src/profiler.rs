use std::{
  collections::HashMap,
  fs::{self, create_dir, create_dir_all},
  path::{Path, PathBuf},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};

use handlebars::Handlebars;
use pyo3::{
  types::{PyAnyMethods, PyDict, PyModule},
  Bound, PyAny, PyResult, Python,
};
use rsa::{
  pkcs1::{self, EncodeRsaPrivateKey},
  BigUint, RsaPrivateKey,
};
use tracing::{span, Level};

use crate::{
  collector::DefaultCollector,
  common::{ExperimentConfig, StorageType, Task},
  constants::MANIFEST,
};

/// A `Profiler` is responsible for managing the benchmarking of tasks within an SGX enclave environment.
///
/// This structure is initialized with various configuration parameters such as the number of threads,
/// enclave sizes, output directory, and a collector for gathering profiling data. It also manages
/// the creation and storage of RSA private keys used for signing the enclave.
///
/// # Fields
///
/// * `private_key_path` - The file path where the RSA private key is stored.
/// * `output_directory` - The directory where profiling results and other output files are stored.
/// * `collector` - An `Arc` wrapped `DefaultCollector` used for collecting profiling data.
/// * `debug` - A boolean flag indicating whether debugging is enabled.
///
/// # Methods
///
/// * `profile` - Initiates the benchmarking of a given task. This method configures the environment,
///   builds and signs the enclave, and executes the task while collecting profiling data.
#[derive(Debug)]
pub struct Profiler {
  private_key_path: PathBuf,
  output_directory: PathBuf,
  collector: Arc<DefaultCollector>,
  debug: bool,
  stop: AtomicBool,
}

impl Profiler {
  pub fn new(
    output_directory: PathBuf,
    debug: bool,
    collector: Arc<DefaultCollector>,
  ) -> Result<Self, std::io::Error> {
    create_dir(&output_directory)?;

    let private_key_path = output_directory.join("private_key.pem");
    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new_with_exp(&mut rng, 3072, &BigUint::new([3].into()))
      .expect("failed to generate a key");

    private_key
      .write_pkcs1_pem_file(&private_key_path, pkcs1::LineEnding::default())
      .unwrap();

    Ok(Profiler {
      private_key_path,
      output_directory,
      debug,
      collector,
      stop: AtomicBool::new(false),
    })
  }

  fn build_and_sign_enclave(
    &self,
    ExperimentConfig {
      program,
      output_path,
      env,
      ..
    }: &ExperimentConfig,
    threads: usize,
    size: &str,
    encrypted_path: &Path,
    untrusted_path: &Path,
    custom_manifest_path: Option<PathBuf>,
  ) -> PyResult<()> {
    Python::with_gil(|py| {
      // variables
      let executable_name = program.file_name().unwrap().to_str().unwrap();
      let executable_path = program.parent().unwrap();
      let manifest_path =
        output_path.join(PathBuf::from(format!("{ }.manifest.sgx", executable_name)));
      let signature_path = output_path.join(format!("{}.sig", executable_name));

      // create env
      let py_env = PyDict::new(py);
      if let Some(ref env_map) = env {
        for (key, val) in env_map {
          py_env.set_item(key, val)?;
        }
      }

      // build enclave
      let gramine = PyModule::import(py, "graminelibos")?;
      let datetime = PyModule::import(py, "datetime")?;
      let manifest = gramine.getattr("Manifest")?;
      let libpal = gramine.getattr("SGX_LIBPAL")?;
      let get_tbssigstruct = gramine.getattr("get_tbssigstruct")?;
      let sign_with_local_key = gramine.getattr("sign_with_local_key")?;

      let args = PyDict::new(py);
      args.set_item("env", py_env)?;
      args.set_item("encrypted_path", encrypted_path)?;
      args.set_item("untrusted_path", untrusted_path)?;
      args.set_item(
        "arch_libdir",
        if cfg!(target_env = "musl") {
          "/lib"
        } else {
          "/lib/x86_64-linux-gnu/"
        },
      )?;
      args.set_item("executable", program.canonicalize()?)?;
      args.set_item("enclave_size", size)?;
      args.set_item("num_threads", threads)?;
      args.set_item("num_threads_sgx", threads + 4)?;
      args.set_item("executable_path", executable_path)?;
      args.set_item("debug", if self.debug { "debug" } else { "none" })?;
      args.set_item(
        "libc",
        if cfg!(target_env = "musl") {
          "musl"
        } else {
          "glibc"
        },
      )?;
      let manifest: Bound<'_, PyAny> = match custom_manifest_path {
        Some(p) => {
          let f = fs::read_to_string(p)?;
          manifest
            .call_method1("from_template", (f, args))?
            .extract()?
        }
        None => manifest
          .call_method1("from_template", (MANIFEST.trim(), args))?
          .extract()?,
      };

      manifest.call_method0("check")?;
      manifest.call_method0("expand_all_trusted_files")?;

      let manifest_data: String = manifest.call_method0("dumps")?.extract()?;
      std::fs::write(&manifest_path, manifest_data)?;

      let today = datetime.getattr("date")?.call_method0("today")?;
      // sign enclave
      let sigstruct: Bound<'_, PyAny> = get_tbssigstruct
        .call1((manifest_path.clone(), today, libpal))?
        .extract()?;

      sigstruct.call_method1(
        "sign",
        (
          sign_with_local_key,
          self.private_key_path.clone().into_os_string(),
        ),
      )?;
      // Save the signature to disk
      let sig_bytes: Vec<u8> = sigstruct
        .call_method0("to_bytes")? // Assuming the sigstruct has a to_bytes method
        .extract()?;

      std::fs::write(&signature_path, sig_bytes)?;
      Ok(())
    })
  }

  #[tracing::instrument(skip(self), level = "info", err)]
  pub fn profile(&self, task: Task) -> Result<(), Box<dyn std::error::Error>> {
    let program_name = task.executable.clone();
    let program_name = program_name.file_name().unwrap().to_str().unwrap();
    let task_path = self.output_directory.join(program_name);

    'outer: for threads in task.num_threads.clone() {
      for enclave_size in &task.enclave_size {
        for storage_type in &task.storage_type {
          if self.stop.load(Ordering::Relaxed) {
            break 'outer;
          }
          let span = span!(
            Level::TRACE,
            "sgx_execution",
            program = program_name,
            threads = threads,
            enclave_size = enclave_size,
            storage_type = storage_type.to_string()
          );
          let _enter = span.enter();
          let experiment_path = task_path.join(format!(
            "gramine-sgx/{}-{}-{}-{}",
            program_name, threads, enclave_size, storage_type
          ));

          // storage
          let paths: Vec<PathBuf> = [
            experiment_path.join(StorageType::Encrypted.to_string()),
            experiment_path.join(StorageType::Untrusted.to_string()),
          ]
          .iter()
          .map(|path| {
            create_dir_all(path).or_else(|e| {
              if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(e);
              }
              Ok(())
            })?;
            path.canonicalize()
          })
          .collect::<Result<Vec<_>, _>>()?;

          let correct_storage_path = match storage_type {
            StorageType::Encrypted => PathBuf::from("/encrypted/"),
            StorageType::Untrusted => PathBuf::from("/untrusted/"),
          };

          let mut experiment_config = build_experiment(
            task.clone(),
            threads,
            &experiment_path,
            &correct_storage_path,
          );

          self.build_and_sign_enclave(
            &experiment_config,
            threads,
            enclave_size,
            &paths[0],
            &paths[1],
            task.custom_manifest_path.clone(),
          )?;
          // since this is a Gramine enclave
          // we need to run the application like gramine-sgx <path-to-manifest>.manifest.sgx <args>
          // for some reasons gramine expects the application name without the .manifest.sgx
          // extension
          let manifest_path = experiment_path
            .join(program_name)
            .to_str()
            .unwrap()
            .to_string();
          experiment_config.args.insert(0, manifest_path);
          experiment_config.program = PathBuf::from("gramine-sgx");
          self.collector.clone().attach(experiment_config)?;
        }
      }
    }

    for threads in task.num_threads.clone() {
      if self.stop.load(Ordering::Relaxed) {
        break;
      }
      let span = span!(
        Level::TRACE,
        "non_sgx_execution",
        program = program_name,
        threads = threads,
      );
      let _enter = span.enter();
      let experiment_path = task_path.join(format!("no-gramine-sgx/{}-{}", program_name, threads));
      let storage_path = experiment_path.join("storage");
      // ensure storage exists
      create_dir_all(&storage_path)?;
      let experiment_config =
        build_experiment(task.clone(), threads, &experiment_path, &storage_path);
      self.collector.clone().attach(experiment_config)?;
    }
    Ok(())
  }

  pub fn stop(&self) {
    self.stop.store(true, Ordering::Relaxed);
  }
}

fn build_experiment(
  Task {
    executable,
    args,
    pre_run_executable,
    pre_run_args,
    post_run_executable,
    post_run_args,
    env,
    ..
  }: Task,
  threads: usize,
  experiment_path: &Path,
  storage_path: &Path,
) -> ExperimentConfig {
  let context = HashMap::from([
    ("num_threads", threads.to_string()),
    (
      "output_directory",
      storage_path.to_string_lossy().into_owned(),
    ),
  ]);
  let handlebars = Handlebars::new();

  let expanded_args: Vec<Vec<String>> = [&args, &pre_run_args, &post_run_args]
    .iter()
    .map(|arg_list| {
      arg_list
        .iter()
        .map(|template_string| handlebars.render_template(template_string, &context))
        .collect::<Result<Vec<String>, _>>()
    })
    .collect::<Result<Vec<_>, _>>()
    .unwrap();

  let [args, pre_run_args, post_run_args] = expanded_args.try_into().unwrap();

  ExperimentConfig {
    program: executable.to_path_buf(),
    pre_run: pre_run_executable.map(|x| (x, pre_run_args)),
    post_run: post_run_executable.map(|x| (x, post_run_args)),
    args,
    output_path: experiment_path.to_path_buf(),
    env: env.map(|c| {
      let mut expanded_env = HashMap::new();
      for (key, val) in c {
        let rendered = handlebars.render_template(&val, &context).unwrap();
        expanded_env.insert(key, rendered);
      }
      expanded_env
    }),
  }
}

#[cfg(test)]
mod test {
  use std::{collections::HashMap, fs::create_dir_all, time::Duration};

  use common::StorageType;
  use profiler::build_experiment;
  use tempfile::TempDir;

  use crate::*;

  #[test]
  fn build_and_sign_enclave_success() {
    let collector = collector::DefaultCollector::new(1, false, Duration::from_millis(100), None);
    let output_directory = TempDir::new().unwrap();
    let profiler = Profiler::new(
      output_directory.path().join("profiler").to_path_buf(),
      false,
      Arc::new(collector),
    )
    .unwrap();

    let task = Task {
      executable: PathBuf::from("/bin/ls"),
      args: vec![],
      pre_run_executable: None,
      pre_run_args: vec![],
      post_run_executable: None,
      post_run_args: vec![],
      env: Some(HashMap::from([(
        "OMP_NUM_THREADS".to_string(),
        "4".to_string(),
      )])),
      num_threads: vec![4],
      enclave_size: vec!["256M".to_string()],
      storage_type: vec![StorageType::Encrypted],
      custom_manifest_path: None,
    };

    let experiment_path = output_directory.path().join("experiment");
    let encrypted_path = experiment_path.join("encrypted");
    let untrusted_path = experiment_path.join("untrusted");
    create_dir_all(&encrypted_path).unwrap();

    let experiment_config = build_experiment(task.clone(), 4, &experiment_path, &encrypted_path);

    profiler
      .build_and_sign_enclave(
        &experiment_config,
        4,
        &task.enclave_size[0],
        &encrypted_path,
        &untrusted_path,
        task.custom_manifest_path.clone(),
      )
      .unwrap();

    let manifest_path = experiment_path.join(format!(
      "{}.manifest.sgx",
      task.executable.file_name().unwrap().to_str().unwrap()
    ));
    let signature_path = experiment_path.join(format!(
      "{}.sig",
      task.executable.file_name().unwrap().to_str().unwrap()
    ));

    assert!(manifest_path.exists(), "Manifest file should exist");
    assert!(signature_path.exists(), "Signature file should exist");
  }

  #[test]
  fn build_experiment_success() {
    let output_directory = TempDir::new().unwrap().path().join("storage");
    let args = vec![
      String::from("{{ output_directory }}"),
      String::from("{{ num_threads }}"),
    ];

    let task = Task {
      executable: PathBuf::from("/path/to/executable"),
      args: args.clone(),
      pre_run_executable: None,
      pre_run_args: vec![],
      post_run_executable: None,
      post_run_args: vec![],
      env: None,
      num_threads: vec![4],
      enclave_size: vec!["256M".to_string()],
      storage_type: vec![StorageType::Encrypted],
      custom_manifest_path: None,
    };

    let experiment_config = build_experiment(task, 4, &output_directory, &output_directory);

    assert_eq!(
      experiment_config.program,
      PathBuf::from("/path/to/executable")
    );
    assert_eq!(
      experiment_config.args,
      vec![
        output_directory.to_string_lossy().into_owned(),
        "4".to_string()
      ]
    );
    assert_eq!(experiment_config.output_path, output_directory);
    assert!(experiment_config.pre_run.is_none());
    assert!(experiment_config.post_run.is_none());
    assert!(experiment_config.env.is_none());
  }
}
