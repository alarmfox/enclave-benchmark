use handlebars::Handlebars;
use pyo3::{
    types::{IntoPyDict, PyAnyMethods, PyModule},
    Bound, PyAny, PyResult, Python,
};
use rsa::{
    pkcs1::{self, EncodeRsaPrivateKey},
    BigUint, RsaPrivateKey,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    fs::{create_dir, create_dir_all, File},
    io::Read,
    path::PathBuf,
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
    globals: GlobalParams,
    tasks: Vec<Task>,
}

#[derive(Deserialize, Debug)]
struct GlobalParams {
    sample_size: u32,
    num_threads: Vec<usize>,
    epc_size: Vec<String>,
    output_directory: PathBuf,
    extra_perf_events: Option<Vec<String>>,
}

#[derive(Debug)]
struct EnclaveMetadata {
    manifest_path: PathBuf,
    signature_path: PathBuf,
    encrypted_path: PathBuf,
    trusted_path: PathBuf,
    tmpfs_path: PathBuf,
    plaintext_path: PathBuf,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
enum StorageType {
    Encrypted,
    Tmpfs,
    Trusted,
    Plaintext,
}

impl Display for StorageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encrypted => write!(f, "{}", "encrypted"),
            Self::Plaintext => write!(f, "{}", "plaintext"),
            Self::Tmpfs => write!(f, "{}", "tmpfs"),
            Self::Trusted => write!(f, "{}", "trusted"),
        }
    }
}
#[derive(Deserialize, Clone, Debug)]
struct Task {
    executable: PathBuf,
    args: Option<Vec<String>>,
    custom_manifest_path: Option<PathBuf>,
    #[serde(default = "default_storage_type")]
    storage_type: Vec<StorageType>,
}

fn default_storage_type() -> Vec<StorageType> {
    vec![StorageType::Plaintext]
}

#[derive(Debug)]
struct Profiler {
    sample_size: u32,
    private_key_path: PathBuf,
    output_directory: PathBuf,
    perf_events: Vec<String>,
    num_threads: Vec<usize>,
    epc_size: Vec<String>,
}

impl Profiler {
    const MANIFEST: &str = r#"
libos.entrypoint = "{{ executable }}"
loader.log_level = "none"

loader.env.OMP_NUM_THREADS = "{{ max_threads }}"
loader.env.LD_LIBRARY_PATH = "/lib"
loader.insecure__use_cmdline_argv = true

fs.mounts = [
  { path = "/lib", uri = "file:{{ gramine.runtimedir() }}" },
  { path = "{{ executable }}", uri = "file:{{ executable }}" },
  { type = "tmpfs", path = "{{ tmpfs_path }}" },
  { path = "{{ trusted_path }}/", uri = "file:{{ trusted_path }}/" },
  { type = "encrypted", path = "{{ encrypted_path }}/", uri = "file:{{ encrypted_path }}/", key_name = "default" },
]

# TODO: generate key
fs.insecure__keys.default = "ffeeddccbbaa99887766554433221100"

sgx.debug = true
sgx.enable_stats = true
sgx.profile.enable = "all"
sgx.profile.with_stack = true
sys.enable_sigterm_injection = true
sgx.enclave_size = "{{ epc_size }}"
sgx.max_threads = {{ max_threads }}
sgx.edmm_enable = false

sgx.trusted_files = [
  "file:{{ executable }}",
  "file:{{ gramine.runtimedir() }}/",
]

sgx.allowed_files = [
  "file::{{ plaintext_path }}/"
]
"#;
    fn new(
        sample_size: u32,
        num_threads: Vec<usize>,
        epc_size: Vec<String>,
        extra_perf_events: Option<Vec<String>>,
        output_directory: PathBuf,
    ) -> Result<Self, std::io::Error> {
        create_dir(&output_directory)?;

        let private_key_path = output_directory.join("private_key.pem");
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new_with_exp(&mut rng, 3072, &BigUint::new([3].into()))
            .expect("failed to generate a key");

        private_key
            .write_pkcs1_pem_file(&private_key_path, pkcs1::LineEnding::default())
            .unwrap();

        let mut perf_events = HashSet::from([
            "user_time".to_string(),
            "system_time".to_string(),
            "duration_time".to_string(),
            "cycles".to_string(),
            "instructions".to_string(),
            "cache-misses".to_string(),
        ]);

        for extra_perf_event in extra_perf_events.unwrap_or_default() {
            perf_events.insert(extra_perf_event);
        }
        let perf_events = Vec::from_iter(perf_events.iter().map(String::from));
        Ok(Profiler {
            private_key_path,
            output_directory,
            sample_size,
            perf_events,
            num_threads,
            epc_size,
        })
    }

    #[tracing::instrument(level = "trace", ret)]
    fn build_and_sign_enclave(
        self: &Self,
        executable: &PathBuf,
        custom_manifest_path: Option<PathBuf>,
        experiment_path: &PathBuf,
        max_threads: &usize,
        epc_size: &String,
    ) -> PyResult<EnclaveMetadata> {
        // ported from https://gramine.readthedocs.io/en/stable/python/api.html
        Python::with_gil(|py| {
            let program_name = executable.file_name().unwrap().to_str().unwrap();
            let manifest_path =
                experiment_path.join(PathBuf::from(format!("{}.manifest.sgx", program_name)));

            let signature_path =
                experiment_path.join(PathBuf::from(format!("{}.sig", program_name)));

            let encrypted_path = experiment_path.join("encrypted");
            create_dir_all(&encrypted_path)?;
            let trusted_path = experiment_path.join("trusted");
            create_dir_all(&trusted_path)?;
            let plaintext_path = experiment_path.join("plaintext");
            create_dir_all(&plaintext_path)?;

            let tmpfs_path = PathBuf::from("/tmp");

            // build enclave
            let gramine = PyModule::import(py, "graminelibos")?;
            let datetime = PyModule::import(py, "datetime")?;
            let manifest = gramine.getattr("Manifest")?;
            let libpal = gramine.getattr("SGX_LIBPAL")?;
            let get_tbssigstruct = gramine.getattr("get_tbssigstruct")?;
            let sign_with_local_key = gramine.getattr("sign_with_local_key")?;
            let args = [
                ("executable", executable.to_str().unwrap()),
                ("epc_size", &epc_size),
                ("max_threads", &max_threads.to_string()),
                ("encrypted_path", encrypted_path.to_str().unwrap()),
                ("plaintext_path", plaintext_path.to_str().unwrap()),
                ("trusted_path", trusted_path.to_str().unwrap()),
                ("tmpfs_path", tmpfs_path.to_str().unwrap()),
            ]
            .into_py_dict(py)?;

            let manifest: Bound<'_, PyAny> = match custom_manifest_path {
                Some(p) => {
                    let mut f = File::open(p)?;
                    let mut buf = String::new();
                    f.read_to_string(&mut buf)?;
                    manifest
                        .call_method1("from_template", (buf.trim(), args))?
                        .extract()?
                }
                None => manifest
                    .call_method1("from_template", (Self::MANIFEST.trim(), args))?
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
            Ok(EnclaveMetadata {
                manifest_path,
                signature_path,
                encrypted_path,
                trusted_path,
                tmpfs_path,
                plaintext_path,
            })
        })
    }

    fn process_args_with_placeholders(
        args: Vec<String>,
        context: &HashMap<&str, String>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let handlebars = Handlebars::new();
        let mut processed_args = Vec::new();

        for arg in args {
            let rendered = handlebars.render_template(&arg, &context)?;
            processed_args.push(rendered);
        }

        Ok(processed_args)
    }

    #[tracing::instrument(skip(self), level = "info", ret)]
    fn profile(self: &Self, task: Task) -> Result<(), Box<dyn std::error::Error>> {
        let program_name = task.executable.file_name().unwrap().to_str().unwrap();
        let task_path = self.output_directory.join(program_name);

        for threads in &self.num_threads {
            for epc in &self.epc_size {
                let experiment_path =
                    task_path.join(format!("gramine-sgx/{}-{}-{}", program_name, threads, epc));
                create_dir_all(&experiment_path)?;

                let enclave_metadata = self.build_and_sign_enclave(
                    &task.executable,
                    task.custom_manifest_path.clone(),
                    &experiment_path,
                    threads,
                    epc,
                )?;

                for storage_type in &task.storage_type {
                    let output_directory = match storage_type {
                        StorageType::Encrypted => &enclave_metadata.encrypted_path,
                        StorageType::Plaintext => &enclave_metadata.plaintext_path,
                        StorageType::Trusted => &enclave_metadata.trusted_path,
                        StorageType::Tmpfs => &enclave_metadata.tmpfs_path,
                    };
                    let context = HashMap::from([
                        ("num_threads", threads.to_string()),
                        (
                            "output_directory",
                            output_directory.to_str().unwrap().to_string(),
                        ),
                    ]);
                    let mut args = Self::process_args_with_placeholders(
                        task.args.clone().unwrap_or_default(),
                        &context,
                    )?;
                    // apparently gramine-sgx adds .manifest.sgx even if the argument already has
                    // .manifest.sgx extention
                    args.insert(
                        0,
                        enclave_metadata.manifest_path.to_str().unwrap().replacen(
                            ".manifest.sgx",
                            "",
                            1,
                        ),
                    );
                    let enclave_path = &experiment_path.join(format!(
                        "{}-{}-{}-{}.csv",
                        program_name, threads, epc, storage_type
                    ));
                    File::create(enclave_path).unwrap();
                    //self.run_with_perf(
                    //    &PathBuf::from("gramine-sgx"),
                    //    args,
                    //    &experiment_path.join(format!(
                    //        "{}-{}-{}-{}.csv",
                    //        program_name, threads, epc, storage_type
                    //    )),
                    //)?;
                }
            }
        }
        for threads in &self.num_threads {
            let experiment_path =
                task_path.join(format!("no-gramine-sgx/{}-{}", program_name, threads));
            create_dir_all(&experiment_path)?;
            let storage_path = experiment_path.join("storage");
            create_dir_all(&storage_path)?;

            let context = HashMap::from([
                ("num_threads", threads.to_string()),
                (
                    "output_directory",
                    storage_path.to_str().unwrap().to_string(),
                ),
            ]);
            let args = Self::process_args_with_placeholders(
                task.args.clone().unwrap_or_default(),
                &context,
            )?;
            self.run_with_perf(
                &task.executable,
                args,
                &experiment_path.join(format!("{}-{}.csv", program_name, threads)),
            )
            .expect("cannot exec app");
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", ret)]
    fn run_with_perf(
        self: &Self,
        executable: &PathBuf,
        executable_args: Vec<String>,
        outfile: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut process = Command::new("perf")
            .arg("stat")
            .arg("--output")
            .arg(outfile)
            .arg("--repeat")
            .arg(self.sample_size.to_string())
            .arg("--field-separator")
            .arg(",")
            .arg("--event")
            .arg(self.perf_events.join(","))
            .arg(executable)
            .args(executable_args)
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
        config.globals.sample_size,
        config.globals.num_threads,
        config.globals.epc_size,
        config.globals.extra_perf_events,
        config.globals.output_directory,
    )?;

    for task in config.tasks {
        profiler.profile(task)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::fs::remove_dir_all;

    use crate::*;
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
        let config = dbg!(config);
        assert_eq!(2, config.tasks.len());
        assert_eq!(3, config.globals.sample_size);
        let args = config.tasks[1].clone().args.unwrap();
        assert_eq!(2, args.len());
        assert_eq!(1, config.globals.num_threads.len());
        assert_eq!(2, config.globals.epc_size.len());
        assert_eq!(1, config.globals.num_threads[0]);
    }

    #[test]
    fn test_simple_profile() {
        let config = toml::from_str::<Config>(
            r#"
            [globals]
            sample_size = 3
            epc_size = ["64M", "128M"]
            num_threads = [1]
            output_directory = "/tmp/test-1"
            extra_perf_events = ["cycles"]
            [[tasks]]
            executable = "/bin/ls"
            "#,
        )
        .unwrap();
        Profiler::new(
            config.globals.sample_size,
            config.globals.num_threads,
            config.globals.epc_size,
            None,
            config.globals.output_directory.clone(),
        )
        .unwrap()
        .run_with_perf(
            &config.tasks[0].executable,
            config.tasks[0].clone().args.unwrap_or_default(),
            &PathBuf::from("/tmp/test-1/test.csv"),
        )
        .unwrap();

        remove_dir_all(config.globals.output_directory).unwrap();
    }

    #[test]
    fn build_and_sign_enclave() {
        let output_directory = PathBuf::from("test");
        let profiler = Profiler::new(
            1,
            vec![1],
            vec!["64M".to_string()],
            None,
            output_directory.clone(),
        )
        .unwrap();

        profiler
            .build_and_sign_enclave(
                &PathBuf::from("/bin/ls"),
                None,
                &output_directory,
                &1,
                &"64M".to_string(),
            )
            .unwrap();
        remove_dir_all(output_directory).unwrap();
    }
    #[test]
    fn test_example_config() {
        let mut buf = String::new();
        let _ = File::open("./examples/example.toml")
            .unwrap()
            .read_to_string(&mut buf)
            .unwrap();
        toml::from_str::<Config>(buf.as_str()).unwrap();
    }
    #[test]
    #[should_panic]
    fn test_invalid_storage_type() {
        toml::from_str::<Config>(
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
            storage_type = ["invalid_storage_type", "tmpfs", "trusted"]
            "#,
        )
        .unwrap();
    }
}
