use pyo3::{
    types::{IntoPyDict, PyAnyMethods, PyDict, PyModule},
    Bound, PyAny, PyResult, Python,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs::{create_dir, create_dir_all, write, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use clap::{arg, command, Error, Parser};
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
    private_key_path: PathBuf,
    sample_size: u32,
    num_threads: Vec<usize>,
    epc_size: Vec<String>,
    tasks: Vec<Task>,
    output_directory: PathBuf,
}

#[derive(Deserialize, Clone, Debug)]
struct Task {
    executable: PathBuf,
    args: Option<Vec<String>>,
}

#[derive(Debug)]
struct Profiler {
    sample_size: u32,
    private_key_path: PathBuf,
    output_directory: PathBuf,
    experiments: Vec<Experiment>,
}

#[derive(Debug)]
struct Experiment {
    threads: usize,
    epc_size: String,
}

impl Profiler {
    const MANIFEST: &str = r#"
libos.entrypoint = "{{ executable }}"
loader.log_level = "none"

loader.env.LD_LIBRARY_PATH = "/lib"
loader.insecure__use_cmdline_argv = true

fs.mounts = [
  { path = "/lib", uri = "file:{{ gramine.runtimedir() }}" },
  { path = "{{ executable }}", uri = "file:{{ executable }}" },
]

sgx.debug = true
sgx.enable_stats = true
sgx.profile.enable = "all"
sgx.profile.with_stack = true
sys.enable_sigterm_injection = true
sgx.enclave_size = "{{ epc_size }}"
sgx.edmm_enable = false

sgx.trusted_files = [
  "file:{{ executable }}",
  "file:{{ gramine.runtimedir() }}/",
]

"#;
    fn new(
        sample_size: u32,
        num_threads: Vec<usize>,
        epc_size: Vec<String>,
        output_directory: PathBuf,
        private_key_path: PathBuf,
    ) -> Result<Self, std::io::Error> {
        let mut experiments: Vec<Experiment> = vec![];

        for &threads in &num_threads {
            for cache in &epc_size {
                experiments.push(Experiment {
                    threads,
                    epc_size: cache.to_string(),
                });
            }
        }
        create_dir_all(&output_directory)?;

        Ok(Profiler {
            private_key_path,
            sample_size,
            experiments,
            output_directory,
        })
    }

    #[tracing::instrument(skip(self), level = "trace", ret)]
    fn build_and_sign_enclave(
        self: &Self,
        base_path: &PathBuf,
        executable: &PathBuf,
        experiment: &Experiment,
    ) -> PyResult<()> {
        let program_name = executable.file_name().unwrap().to_str().unwrap();
        let manifest_path = base_path.join(PathBuf::from(format!("{}.manifest.sgx", program_name)));

        let sig_path = base_path.join(PathBuf::from(format!("{}.sig", program_name)));

        Python::with_gil(|py| {
            // build enclave
            let gramine = PyModule::import(py, "graminelibos")?;
            let datetime = PyModule::import(py, "datetime")?;
            let manifest = gramine.getattr("Manifest")?;
            let libpal = gramine.getattr("SGX_LIBPAL")?;
            let get_tbssigstruct = gramine.getattr("get_tbssigstruct")?;
            let sign_with_local_key = gramine.getattr("sign_with_local_key")?;
            let args = [
                ("executable", executable.to_str().unwrap()),
                ("epc_size", &experiment.epc_size),
            ]
            .into_py_dict(py)?;

            let manifest: Bound<'_, PyAny> = manifest
                .call_method1("from_template", (Self::MANIFEST.trim(), args))?
                .extract()?;

            manifest.call_method0("check")?;
            //let expanded: Bound<'_, PyAny> = manifest
            //    .call_method0("expand_all_trusted_files")?
            //    .extract()?;

            let manifest_data: String = manifest.call_method0("dumps")?.extract()?;
            println!("{}", manifest_path.to_str().unwrap());
            std::fs::write(&manifest_path, manifest_data)?;

            let today = datetime.getattr("date")?.call_method0("today")?;
            // sign enclave
            let sigstruct: Bound<'_, PyAny> = get_tbssigstruct
                .call1((manifest_path, today, libpal))?
                .extract()?;

            sigstruct.call_method1(
                "sign",
                (sign_with_local_key, self.private_key_path.to_str().unwrap()),
            )?;
            // Save the signature to disk
            let sig_bytes: Vec<u8> = sigstruct
                .call_method0("to_bytes")? // Assuming the sigstruct has a to_bytes method
                .extract()?;

            std::fs::write(&sig_path, sig_bytes)?;
            Ok(())
        })
    }

    #[tracing::instrument(level = "info", ret)]
    fn profile(self: &Self, task: Task) -> Result<(), Box<dyn std::error::Error>> {
        // enclave only
        let program_name = task.executable.file_name().unwrap().to_str().unwrap();
        let task_path = self.output_directory.join(program_name);
        create_dir(&task_path)?;
        for experiment in &self.experiments {
            let experiment_path =
                task_path.join(format!("th{}-{}", experiment.threads, experiment.epc_size));
            create_dir(&experiment_path)?;
            self.build_and_sign_enclave(&experiment_path, &task.executable, experiment)?;
        }

        Ok(())
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
        config.epc_size,
        config.output_directory,
        config.private_key_path,
    )?;

    for task in config.tasks {
        profiler.run_outside_enclave(task)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::fs::remove_dir_all;

    use rsa::{
        pkcs1::{self, EncodeRsaPrivateKey},
        BigUint, RsaPrivateKey,
    };

    use crate::*;
    #[test]
    fn test_parse_config() {
        let config = toml::from_str::<Config>(
            r#"
            sample_size = 3
            epc_size = ["64M", "128M"]
            num_threads = [1]
            output_directory = "/test"
            private_key_path = "/tmp/test"
            [[tasks]]
            executable = "/bin/ls"
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            "#,
        )
        .unwrap();
        let config = dbg!(config);
        assert_eq!(2, config.tasks.len());
        assert_eq!(3, config.sample_size);
        let args = config.tasks[1].clone().args.unwrap();
        assert_eq!(2, args.len());
        assert_eq!(1, config.num_threads.len());
        assert_eq!(2, config.epc_size.len());
        assert_eq!(1, config.num_threads[0]);
    }

    #[test]
    fn test_simple_profile() {
        let config = toml::from_str::<Config>(
            r#"
            sample_size = 3
            epc_size = ["64M", "128M"]
            num_threads = [1]
            output_directory = "/tmp/test"
            private_key_path = "/tmp/test/key"
            [[tasks]]
            executable = "/bin/ls"
            "#,
        )
        .unwrap();
        Profiler::new(
            config.sample_size,
            config.num_threads,
            config.epc_size,
            config.output_directory.clone(),
            config.private_key_path,
        )
        .unwrap()
        .run_outside_enclave(config.tasks[0].clone())
        .unwrap();

        remove_dir_all(config.output_directory).unwrap();
    }

    #[test]
    fn build_and_sign_enclave() {
        let output_directory = PathBuf::from("test");
        let private_key_path = output_directory.join("private_key.pem");
        create_dir_all(&output_directory).unwrap();
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new_with_exp(&mut rng, 3072, &BigUint::new([3].into()))
            .expect("failed to generate a key");

        private_key
            .write_pkcs1_pem_file(&private_key_path, pkcs1::LineEnding::default())
            .unwrap();
        let profiler = Profiler::new(
            1,
            vec![1],
            vec!["64M".to_string()],
            output_directory.clone(),
            private_key_path,
        )
        .unwrap();

        let experiment = Experiment {
            threads: 1,
            epc_size: "64M".into(),
        };
        profiler
            .build_and_sign_enclave(&output_directory, &PathBuf::from("/bin/ls"), &experiment)
            .unwrap();
        remove_dir_all(output_directory).unwrap();
    }
}
