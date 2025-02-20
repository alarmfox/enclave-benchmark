use std::{
  collections::HashMap,
  fs::{create_dir, create_dir_all, File},
  io::Read,
  path::{Path, PathBuf},
  sync::Arc,
};

use handlebars::Handlebars;
use pyo3::{
  types::{IntoPyDict, PyAnyMethods, PyModule},
  Bound, PyAny, PyResult, Python,
};
use rsa::{
  pkcs1::{self, EncodeRsaPrivateKey},
  BigUint, RsaPrivateKey,
};

use crate::{
  collector::DefaultCollector,
  common::{StorageType, Task},
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
/// * `num_threads` - A vector specifying the number of threads to be used for each profiling task.
/// * `enclave_size` - A vector specifying the sizes of the enclaves to be used for profiling.
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
}

#[derive(Debug, Clone)]
struct GramineMetadata {
  manifest_path: PathBuf,
  encrypted_path: PathBuf,
  tmpfs_path: PathBuf,
  untrusted_path: PathBuf,
}

impl Profiler {
  pub fn new(
    output_directory: PathBuf,
    debug: bool,
    collector: DefaultCollector,
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
      collector: Arc::new(collector),
    })
  }

  #[tracing::instrument(level = "debug", skip(self), err)]
  fn build_and_sign_enclave(
    &self,
    executable: &PathBuf,
    experiment_path: &PathBuf,
    num_threads: &usize,
    enclave_size: &str,
    custom_manifest_path: Option<PathBuf>,
  ) -> PyResult<GramineMetadata> {
    // ported from https://gramine.readthedocs.io/en/stable/python/api.html
    Python::with_gil(|py| {
      let executable_name = executable.file_name().unwrap().to_str().unwrap();
      let executable_path = executable.parent().unwrap();
      let manifest_path =
        experiment_path.join(PathBuf::from(format!("{}.manifest.sgx", executable_name)));

      let signature_path = experiment_path.join(PathBuf::from(format!("{}.sig", executable_name)));

      let encrypted_path = experiment_path.join("encrypted");
      let untrusted_path = experiment_path.join("untrusted");

      for path in [&encrypted_path, &untrusted_path] {
        create_dir_all(path)?;
      }
      let encrypted_path = encrypted_path.canonicalize().unwrap();
      let untrusted_path = untrusted_path.canonicalize().unwrap();

      let tmpfs_path = PathBuf::from("/tmp");

      // build enclave
      let gramine = PyModule::import(py, "graminelibos")?;
      let datetime = PyModule::import(py, "datetime")?;
      let manifest = gramine.getattr("Manifest")?;
      let libpal = gramine.getattr("SGX_LIBPAL")?;
      let get_tbssigstruct = gramine.getattr("get_tbssigstruct")?;
      let sign_with_local_key = gramine.getattr("sign_with_local_key")?;

      let args = [
        (
          "arch_libdir",
          if cfg!(target_env = "musl") {
            "/lib"
          } else {
            "/lib/x86_64-linux-gnu/"
          },
        ),
        ("executable", executable.to_str().unwrap()),
        ("enclave_size", enclave_size),
        ("num_threads", &num_threads.to_string()),
        ("num_threads_sgx", &(num_threads + 4).to_string()),
        ("encrypted_path", encrypted_path.to_str().unwrap()),
        ("untrusted_path", untrusted_path.to_str().unwrap()),
        ("tmpfs_path", tmpfs_path.to_str().unwrap()),
        (
          "start_directory",
          manifest_path.parent().unwrap().to_str().unwrap(),
        ),
        ("executable_path", executable_path.to_str().unwrap()),
        ("debug", if self.debug { "debug" } else { "none" }),
        (
          "libc",
          if cfg!(target_env = "musl") {
            "musl"
          } else {
            "glibc"
          },
        ),
      ]
      .into_py_dict(py)?;

      let manifest: Bound<'_, PyAny> = match custom_manifest_path {
        Some(p) => {
          let mut f = File::open(p)?;
          let mut buf = String::new();
          let n = f.read_to_string(&mut buf)?;
          manifest
            .call_method1("from_template", (buf[..n].trim(), args))?
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
        (sign_with_local_key, self.private_key_path.to_str().unwrap()),
      )?;
      // Save the signature to disk
      let sig_bytes: Vec<u8> = sigstruct
        .call_method0("to_bytes")? // Assuming the sigstruct has a to_bytes method
        .extract()?;

      std::fs::write(&signature_path, sig_bytes)?;
      Ok(GramineMetadata {
        manifest_path,
        encrypted_path: PathBuf::from("/encrypted/"),
        tmpfs_path,
        untrusted_path: PathBuf::from("/untrusted/"),
      })
    })
  }

  #[allow(clippy::type_complexity)]
  fn build_and_expand_args(
    args: Vec<String>,
    pre_args: Vec<String>,
    post_args: Vec<String>,
    num_threads: usize,
    storage_type: &StorageType,
    fallback_storage_path: &Path,
    gramine_metadata: Option<GramineMetadata>,
  ) -> Result<(Vec<String>, Vec<String>, Vec<String>), Box<dyn std::error::Error>> {
    // detect storage type if in sgx
    // otherwise a simple directory is returned
    let output_directory = match gramine_metadata.clone() {
      Some(metadata) => match storage_type {
        StorageType::Encrypted => metadata.encrypted_path,
        StorageType::Untrusted => metadata.untrusted_path,
        StorageType::Tmpfs => metadata.tmpfs_path,
      },
      None => fallback_storage_path.to_path_buf(),
    };

    // expand args
    let context = HashMap::from([
      ("num_threads", num_threads.to_string()),
      (
        "output_directory",
        output_directory.to_str().unwrap().to_string(),
      ),
    ]);
    let handlebars = Handlebars::new();

    let mut args: Vec<Vec<String>> = [&args, &pre_args, &post_args]
      .iter()
      .map(|arg_list| {
        arg_list
          .iter()
          .map(|template_string| handlebars.render_template(template_string, &context))
          .collect::<Result<Vec<String>, _>>()
      })
      .collect::<Result<Vec<_>, _>>()?;

    if let Some(metadata) = gramine_metadata {
      args[0].insert(
        0,
        metadata
          .manifest_path
          .to_str()
          .unwrap()
          .replacen(".manifest.sgx", "", 1),
      );
    }

    Ok((args.remove(0), args.remove(0), args.remove(0)))
  }

  #[tracing::instrument(skip(self), level = "info", err)]
  pub fn profile(&self, task: Task) -> Result<(), Box<dyn std::error::Error>> {
    let program_name = task.executable.file_name().unwrap().to_str().unwrap();
    let task_path = self.output_directory.join(program_name);
    let collector = self.collector.clone();

    let process_task = |program: &PathBuf,
                        num_threads: &usize,
                        sgx_metadata: Option<(&str, GramineMetadata)>|
     -> Result<(), Box<dyn std::error::Error>> {
      let experiment_path = match sgx_metadata {
        Some((size, _)) => task_path.join(format!(
          "gramine-sgx/{}-{}-{}",
          program_name, num_threads, size
        )),
        None => task_path.join(format!("no-gramine-sgx/{}-{}", program_name, num_threads)),
      };
      create_dir_all(&experiment_path)?;
      // create plaintext storage path
      let fallback_storage_path = experiment_path.join("storage");

      let storage_types = if sgx_metadata.is_some() {
        task.storage_type.clone()
      } else {
        create_dir_all(&fallback_storage_path)?;
        vec![StorageType::Untrusted]
      };

      for storage_type in &storage_types {
        let (args, pre_args, post_args) = Self::build_and_expand_args(
          task.args.clone(),
          task.pre_run_args.clone(),
          task.post_run_args.clone(),
          *num_threads,
          storage_type,
          &fallback_storage_path,
          sgx_metadata.clone().map(|x| x.1),
        )?;

        let pre_task = task.pre_run_executable.clone().map(|e| (e, pre_args));
        let post_task = task.post_run_executable.clone().map(|e| (e, post_args));

        let (program, args, result_path) = match sgx_metadata {
          Some((size, _)) => (
            PathBuf::from("gramine-sgx"),
            args,
            experiment_path.join(format!(
              "{}-{}-{}-{}",
              program_name, num_threads, size, storage_type
            )),
          ),
          None => (
            program.clone(),
            args,
            experiment_path.join(format!("{}-{}-{}", program_name, num_threads, storage_type)),
          ),
        };

        collector
          .clone()
          .attach(program, args, pre_task, post_task, &result_path)?;
      }
      Ok(())
    };

    let num_threads = if let Some(threads) = task.num_threads {
      threads
    } else {
      vec![1]
    };

    for num_threads in &num_threads {
      for enclave_size in &task.enclave_size {
        let gramine_metadata = self.build_and_sign_enclave(
          &task.executable,
          &task_path.join(format!(
            "gramine-sgx/{}-{}-{}",
            program_name, num_threads, enclave_size
          )),
          num_threads,
          enclave_size,
          task.custom_manifest_path.clone(),
        )?;
        process_task(
          &task.executable,
          num_threads,
          Some((enclave_size, gramine_metadata)),
        )?;
      }
      process_task(&task.executable, num_threads, None)?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use std::time::Duration;

  use common::StorageType;
  use profiler::GramineMetadata;
  use tempfile::TempDir;

  use crate::*;

  #[test]
  fn build_and_sign_enclave() {
    let collector = collector::DefaultCollector::new(1, false, Duration::from_millis(100), None);
    let output_directory = TempDir::new().unwrap();
    let profiler = Profiler::new(
      output_directory.path().join("profiler").to_path_buf(),
      false,
      collector,
    )
    .unwrap();

    profiler
      .build_and_sign_enclave(
        &PathBuf::from("/bin/ls"),
        &output_directory.path().to_path_buf(),
        &1,
        "64M",
        None,
      )
      .unwrap();
  }
  #[test]
  fn example_configs() {
    let mut buf = String::new();
    let examples = [
      "examples/full.toml",
      "examples/simple.toml",
      "examples/iobound.toml",
      "examples/minimal.toml",
    ];
    for file in examples {
      let n = File::open(PathBuf::from(file))
        .unwrap()
        .read_to_string(&mut buf)
        .unwrap();
      toml::from_str::<Config>(&buf[..n]).unwrap();
      buf.clear();
    }
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
            storage_type = ["invalid_storage_type", "tmpfs"]
            enclave_size = ["64M", "128M"]
            "#,
    )
    .unwrap();
  }

  #[test]
  fn build_and_expand_args() {
    let output_directory = TempDir::new().unwrap().path().join("storage");
    let args = vec![
      String::from("{{ output_directory }}"),
      String::from("{{ num_threads }}"),
    ];
    let args = Profiler::build_and_expand_args(
      args,
      vec![],
      vec![],
      1,
      &StorageType::Untrusted,
      &output_directory,
      None,
    )
    .unwrap();

    assert_eq!(args.0[0], output_directory.clone().to_str().unwrap());
    assert_eq!(args.0[1], String::from("1"));

    let args = vec![
      String::from("{{ output_directory }}"),
      String::from("{{ num_threads }}"),
    ];

    let gramine_metadata = GramineMetadata {
      manifest_path: output_directory.join("app.manifest.sgx"),
      encrypted_path: output_directory.join("encrypted_path"),
      untrusted_path: output_directory.join("plaintext_path"),
      tmpfs_path: output_directory.join("tmpfs_path"),
    };
    let args = Profiler::build_and_expand_args(
      args,
      vec![],
      vec![],
      1,
      &StorageType::Encrypted,
      &output_directory,
      Some(gramine_metadata.clone()),
    )
    .unwrap();

    assert_eq!(
      args.0[0],
      gramine_metadata
        .manifest_path
        .to_str()
        .unwrap()
        .to_string()
        .replacen(".manifest.sgx", "", 1)
    );
    assert_eq!(
      args.0[1],
      output_directory.join("encrypted_path").to_str().unwrap()
    );
    assert_eq!(args.0[2], String::from("1"));
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
            storage_type = ["tmpfs"] 
            enclave_size = ["64M", "128M"]
            "#,
    )
    .unwrap();

    assert_eq!(config.tasks.len(), 2);
    assert_eq!(config.tasks[0].storage_type.len(), 1);
    assert_eq!(config.tasks[0].storage_type[0], StorageType::Untrusted);
    assert_eq!(config.tasks[1].storage_type[0], StorageType::Tmpfs);
  }
}
