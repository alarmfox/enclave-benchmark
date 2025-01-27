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
    fmt::{Debug, Display},
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

#[derive(Debug, Clone)]
struct GramineMetadata {
    manifest_path: PathBuf,
    encrypted_path: PathBuf,
    trusted_path: PathBuf,
    tmpfs_path: PathBuf,
    plaintext_path: PathBuf,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
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
            Self::Encrypted => write!(f, "encrypted"),
            Self::Plaintext => write!(f, "plaintext"),
            Self::Tmpfs => write!(f, "tmpfs"),
            Self::Trusted => write!(f, "trusted"),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
struct Task {
    executable: PathBuf,
    args: Option<Vec<String>>,
    custom_manifest_path: Option<PathBuf>,
    #[serde(
        default = "default_storage_type",
        deserialize_with = "deserialize_storage_type"
    )]
    storage_type: Vec<StorageType>,
}

fn default_storage_type() -> Vec<StorageType> {
    vec![StorageType::Plaintext]
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
#[derive(Debug)]
struct Profiler {
    private_key_path: PathBuf,
    output_directory: PathBuf,
    num_threads: Vec<usize>,
    epc_size: Vec<String>,
    collector: Box<dyn Collector + 'static>,
}

impl Profiler {
    const MANIFEST: &str = r#"
libos.entrypoint = "{{ executable }}"
loader.log_level = "none"

loader.env.OMP_NUM_THREADS = "{{ num_threads }}"
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
sgx.max_threads = {{ num_threads }}
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
        num_threads: Vec<usize>,
        epc_size: Vec<String>,
        output_directory: PathBuf,
        collector: Box<dyn Collector + 'static>,
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
            num_threads,
            epc_size,
            collector,
        })
    }

    #[tracing::instrument(level = "trace", skip(self), ret)]
    fn build_and_sign_enclave(
        &self,
        executable: &PathBuf,
        experiment_path: &PathBuf,
        num_threads: &usize,
        epc_size: &String,
        custom_manifest_path: Option<PathBuf>,
    ) -> PyResult<GramineMetadata> {
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
                ("epc_size", epc_size),
                ("num_threads", &num_threads.to_string()),
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
            Ok(GramineMetadata {
                manifest_path,
                encrypted_path,
                trusted_path,
                tmpfs_path,
                plaintext_path,
            })
        })
    }

    fn build_and_expand_args(
        args: Vec<String>,
        num_threads: usize,
        fallback_storage_path: PathBuf,
        storage_type: Option<StorageType>,
        gramine_metadata: Option<GramineMetadata>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // detect storage type if in sgx
        // otherwise a simple directory is returned
        let output_directory = match gramine_metadata.clone() {
            Some(metadata) => match storage_type {
                Some(StorageType::Encrypted) => metadata.encrypted_path,
                Some(StorageType::Plaintext) => metadata.plaintext_path,
                Some(StorageType::Trusted) => metadata.trusted_path,
                Some(StorageType::Tmpfs) => metadata.tmpfs_path,
                None => panic!("gramine sgx must have a storage type"),
            },
            None => fallback_storage_path,
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
        let mut processed_args = Vec::new();

        for arg in args {
            let rendered = handlebars.render_template(&arg, &context)?;
            processed_args.push(rendered);
        }

        let args = match gramine_metadata {
            Some(metadata) => {
                processed_args.insert(
                    0,
                    metadata
                        .manifest_path
                        .to_str()
                        .unwrap()
                        .replacen(".manifest.sgx", "", 1),
                );
                processed_args
            }
            None => processed_args,
        };

        Ok(args)
    }

    #[tracing::instrument(skip(self), level = "info", ret)]
    fn profile(&mut self, task: Task) -> Result<(), Box<dyn std::error::Error>> {
        let program_name = task.executable.file_name().unwrap().to_str().unwrap();
        let task_path = self.output_directory.join(program_name);

        for num_threads in &self.num_threads {
            for epc_size in &self.epc_size {
                let experiment_path = task_path.join(format!(
                    "gramine-sgx/{}-{}-{}",
                    program_name, num_threads, epc_size
                ));
                create_dir_all(&experiment_path)?;

                let gramine_metadata = self.build_and_sign_enclave(
                    &task.executable,
                    &experiment_path,
                    num_threads,
                    epc_size,
                    task.custom_manifest_path.clone(),
                )?;

                for storage_type in &task.storage_type {
                    let args = Self::build_and_expand_args(
                        task.args.clone().unwrap_or_default(),
                        *num_threads,
                        gramine_metadata.clone().plaintext_path,
                        Some(storage_type.clone()),
                        Some(gramine_metadata.clone()),
                    )?;
                    let result_path = &experiment_path.join(format!(
                        "{}-{}-{}-{}",
                        program_name, num_threads, epc_size, storage_type
                    ));
                    self.collector
                        .attach(PathBuf::from("gramine-sgx"), args, result_path)?;
                }
            }
        }
        for num_threads in &self.num_threads {
            let experiment_path =
                task_path.join(format!("no-gramine-sgx/{}-{}", program_name, num_threads));
            let storage_path = experiment_path.join("storage");
            create_dir_all(&storage_path)?;

            let args = Self::build_and_expand_args(
                task.args.clone().unwrap_or_default(),
                *num_threads,
                storage_path,
                None,
                None,
            )?;

            self.collector
                .attach(task.executable.clone(), args, &experiment_path)?;
        }
        Ok(())
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

    let mut profiler = Profiler::new(
        config.globals.num_threads,
        config.globals.epc_size,
        config.globals.output_directory,
        Box::new(FullMetricsCollector::new(
            config.globals.sample_size,
            config.globals.extra_perf_events,
        )),
    )?;

    for task in config.tasks {
        profiler.profile(task)?;
    }
    Ok(())
}

trait Collector {
    fn attach(
        &mut self,
        _program: PathBuf,
        _args: Vec<String>,
        _output_directory: &Path,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

struct FullMetricsCollector {
    sample_size: u32,
    strace_cmd: Command,
    perf_cmd: Command,
}

impl FullMetricsCollector {
    const DEFAULT_PERF_EVENTS: [&str; 28] = [
        "user_time",
        "system_time",
        "duration_time",
        "cycles",
        "instructions",
        "cache-misses",
        "L1-dcache-loads",
        "L1-dcache-load-misses",
        "L1-dcache-prefetches",
        "L1-icache-loads",
        "L1-icache-load-misses",
        "dTLB-loads",
        "dTLB-load-misses",
        "iTLB-loads",
        "iTLB-load-misses",
        "branch-loads",
        "branch-load-misses",
        "branch-instructions",
        "branch-misses",
        "cache-misses",
        "cache-references",
        "cpu-cycles",
        "instructions",
        "stalled-cycles-frontend",
        "branch-misses",
        "cache-misses",
        "cpu-cycles",
        "page-faults",
    ];
    fn new(sample_size: u32, extra_perf_events: Option<Vec<String>>) -> Self {
        Self {
            sample_size,
            strace_cmd: {
                let mut cmd = Command::new("strace");
                cmd.arg("-ttt");
                cmd.stdout(Stdio::null());
                cmd.stderr(Stdio::null());
                cmd
            },

            perf_cmd: {
                let mut perf_events: HashSet<String> =
                    HashSet::from_iter(Self::DEFAULT_PERF_EVENTS.iter().map(|v| v.to_string()));
                for extra_perf_event in extra_perf_events.unwrap_or_default() {
                    perf_events.insert(extra_perf_event);
                }
                let perf_events = Vec::from_iter(perf_events.iter().map(String::from));
                let mut cmd = Command::new("perf");
                cmd.arg("stat");
                cmd.arg("--field-separator");
                cmd.arg(",");
                cmd.arg("--event");
                cmd.arg(perf_events.join(","));
                cmd.stdout(Stdio::null());
                cmd.stderr(Stdio::null());
                cmd
            },
        }
    }
}

impl Collector for FullMetricsCollector {
    fn attach(
        &mut self,
        program: PathBuf,
        args: Vec<String>,
        output_directory: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for n in 1..self.sample_size + 1 {
            let experiment_directory = output_directory.join(PathBuf::from(n.to_string()));
            create_dir_all(&experiment_directory)?;
            if program.to_str().unwrap() == "gramine-sgx" {
                File::create(experiment_directory.join("strace.log"))?;
                File::create(experiment_directory.join("perf.csv"))?;
                continue;
            }
            let cmd = Command::new(&program)
                .args(args.clone())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            match cmd {
                Ok(child) => {
                    let perf_child = self
                        .perf_cmd
                        .arg("--output")
                        .arg(experiment_directory.join("perf.csv"))
                        .arg("--pid")
                        .arg(child.id().to_string())
                        .spawn()?;

                    let strace_child = self
                        .strace_cmd
                        .arg("--output")
                        .arg(experiment_directory.join("strace.log"))
                        .arg("--attach")
                        .arg(child.id().to_string())
                        .spawn()?;

                    println!(
                        "strace pid = {}; perf pid = {}; program pid = {}",
                        strace_child.id(),
                        perf_child.id(),
                        child.id()
                    );

                    for mut child in [child, perf_child, strace_child] {
                        match child.wait() {
                            Ok(status) => match status.code().unwrap() {
                                0 => {}
                                n => {
                                    // not panicking because some tasks are too fast to be measured
                                    // so they terminate before strace and perf
                                    println!(
                                        "process with pid {} exited with code {}",
                                        child.id(),
                                        n,
                                    )
                                }
                            },
                            Err(e) => panic!("error waiting {}", e),
                        }
                    }
                }
                Err(e) => panic!("process exited with error {}", e),
            }
        }
        Ok(())
    }
}

impl Debug for dyn Collector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Collector debug")
    }
}

#[cfg(test)]
mod test {
    use tempfile::TempDir;
    struct DummyCollector;

    impl Collector for DummyCollector {
        fn attach(
            &mut self,
            _program: PathBuf,
            _args: Vec<String>,
            _output_directory: &Path,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }

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
    fn build_and_sign_enclave() {
        let output_directory = TempDir::new().unwrap();
        let profiler = Profiler::new(
            vec![1],
            vec!["64M".to_string()],
            output_directory.path().join("profiler").to_path_buf(),
            Box::new(DummyCollector),
        )
        .unwrap();

        profiler
            .build_and_sign_enclave(
                &PathBuf::from("/bin/ls"),
                &output_directory.path().to_path_buf(),
                &1,
                &"64M".to_string(),
                None,
            )
            .unwrap();
    }
    #[test]
    fn test_example_configs() {
        let mut buf = String::new();
        let examples = ["examples/full.toml", "examples/simple.toml"];
        for file in examples {
            let _ = File::open(PathBuf::from(file))
                .unwrap()
                .read_to_string(&mut buf)
                .unwrap();
            toml::from_str::<Config>(buf.as_str()).unwrap();
            buf.clear();
        }
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

    #[test]
    fn test_build_and_expand_args() {
        let output_directory = TempDir::new().unwrap().path().join("storage");
        let args = vec![
            String::from("{{ output_directory }}"),
            String::from("{{ num_threads }}"),
        ];
        let args =
            Profiler::build_and_expand_args(args, 1, output_directory.clone(), None, None).unwrap();

        assert_eq!(args[0], output_directory.clone().to_str().unwrap());
        assert_eq!(args[1], String::from("1"));

        let args = vec![
            String::from("{{ output_directory }}"),
            String::from("{{ num_threads }}"),
        ];

        let gramine_metadata = GramineMetadata {
            manifest_path: output_directory.join("app.manifest.sgx"),
            encrypted_path: output_directory.join("encrypted_path"),
            plaintext_path: output_directory.join("plaintext_path"),
            trusted_path: output_directory.join("trusted_path"),
            tmpfs_path: output_directory.join("tmpfs_path"),
        };
        let args = Profiler::build_and_expand_args(
            args,
            1,
            output_directory.join("fallback"),
            Some(StorageType::Encrypted),
            Some(gramine_metadata.clone()),
        )
        .unwrap();

        assert_eq!(
            args[0],
            gramine_metadata
                .manifest_path
                .to_str()
                .unwrap()
                .to_string()
                .replacen(".manifest.sgx", "", 1)
        );
        assert_eq!(
            args[1],
            output_directory.join("encrypted_path").to_str().unwrap()
        );
        assert_eq!(args[2], String::from("1"));
    }

    #[test]
    #[should_panic]
    fn test_missing_storage_for_sgx() {
        let output_directory = TempDir::new().unwrap().path().join("storage");
        let args = vec![
            String::from("{{ output_directory }}"),
            String::from("{{ num_threads }}"),
        ];

        let gramine_metadata = GramineMetadata {
            manifest_path: output_directory.join("app.manifest.sgx"),
            encrypted_path: output_directory.join("encrypted_path"),
            plaintext_path: output_directory.join("plaintext_path"),
            trusted_path: output_directory.join("trusted_path"),
            tmpfs_path: output_directory.join("tmpfs_path"),
        };
        Profiler::build_and_expand_args(
            args,
            1,
            output_directory.join("fallback"),
            None,
            Some(gramine_metadata.clone()),
        )
        .unwrap();
    }

    #[test]
    fn test_default_storage_type() {
        let config = toml::from_str::<Config>(
            r#"
            [globals]
            sample_size = 3
            epc_size = ["64M", "128M"]
            num_threads = [1]
            output_directory = "/test"
            [[tasks]]
            executable = "/bin/ls"
            storage_type = []
            [[tasks]]
            executable = "/bin/ls"
            args = ["-l", "-a"]
            storage_type = ["tmpfs", "trusted"]
            "#,
        )
        .unwrap();

        assert_eq!(config.tasks[0].storage_type.len(), 1);
        assert_eq!(config.tasks[0].storage_type[0], StorageType::Plaintext);
    }

    #[test]
    fn test_collector() {
        let output_directory = TempDir::new().unwrap();
        let mut collector = FullMetricsCollector::new(2, None);
        collector
            .attach(
                PathBuf::from("/bin/dd"),
                vec![
                    "if=/dev/zero".into(),
                    "of=/dev/null".into(),
                    "count=10000".into(),
                ],
                &output_directory.path().to_path_buf(),
            )
            .unwrap();

        for i in 1..3 {
            assert!(output_directory
                .path()
                .join(format!("{}/strace.log", i))
                .is_file());
            assert!(output_directory
                .path()
                .join(format!("{}/perf.csv", i))
                .is_file());
        }
    }
}
