Code Documentation
==================

src/common.rs
-------------

StorageType
~~~~~~~~~~~

StorageType defines the types of storage that can be used.

# Variants

- **Encrypted** - Represents encrypted storage.
- **Tmpfs** - Represents temporary file system storage.
- **Untrusted** - Represents untrusted storage.

.. collapse:: Show Code

   .. code-block:: rust

      pub enum StorageType {
        Encrypted,
        Tmpfs,
        Untrusted,
      }

GlobalParams
~~~~~~~~~~~~

GlobalParams holds the configuration parameters for the global settings of the application.

# Fields

- **sample_size** - Specifies the number of times each experiment is repeated.
- **num_threads** - A vector specifying the number of threads to be used in each experiment.
- **enclave_size** - A vector of strings representing the possible enclave memory sizes. Each experiment will be run with every listed size.
- **output_directory** - The directory where benchmark results and outputs are stored. This variable can be referenced in task configurations using {{ output_directory }}.
- **extra_perf_events** - An optional vector of strings for additional performance monitoring events to be collected.
- **debug** - A boolean flag for enabling debug logging for more detailed output. Defaults to false.
- **deep_trace** - A boolean flag for enabling an extra experiment with tracing enabled. This can be very slow. Defaults to false.
- **energy_sample_interval** - The interval for energy sampling, deserialized using deserialize_duration. Must be specified with a time unit (e.g., "250ms" for 250 milliseconds). Defaults to 500 milliseconds.

.. collapse:: Show Code

   .. code-block:: rust

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

Task
~~~~

Task represents a task to be executed, including its configuration and associated scripts.

# Fields

* **executable** - The path to the executable file for the task.
* **args** - A vector of arguments to be passed to the executable. Defaults to an empty vector.
* **custom_manifest_path** - An optional path to a custom manifest file.
* **storage_type** - A vector of storage types, deserialized using **deserialize_storage_type**. Defaults to **[StorageType::Untrusted]**.
* **pre_run_executable** - An optional path to an executable to run before the main task.
* **pre_run_args** - A vector of arguments for the pre-run executable. Defaults to an empty vector.
* **post_run_executable** - An optional path to an executable to run after the main task.
* **post_run_args** - A vector of arguments for the post-run executable. Defaults to an empty vector.

.. collapse:: Show Code

   .. code-block:: rust

      pub struct Task {
        pub executable: PathBuf,
      
        #[serde(default)]
        pub args: Vec<String>,
      
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


src/constants.rs
----------------

MANIFEST
~~~~~~~~

The Gramine manifest configuration for an enclave application.

This manifest defines various settings and parameters for running an application
within a Gramine enclave. It includes configuration for the entry point, logging,
environment variables, file system mounts, security settings, and SGX-specific options.

# Variables

- `{{ executable }}`: The path to the executable that serves as the entry point for the application.

- `{{ debug }}`: The log level for the loader, which determines the verbosity of logging output.

- `{{ num_threads }}`: The number of OpenMP threads to be used by the application, set via the `OMP_NUM_THREADS` environment variable.

- `{{ gramine.runtimedir() }}`: The directory path where Gramine runtime libraries are located, used for mounting the `/lib` path.

- `{{ arch_libdir }}`: The architecture-specific library directory, used for mounting and trusted file paths.

- `{{ tmpfs_path }}`: The path for a temporary filesystem (tmpfs) mount within the enclave.

- `{{ encrypted_path }}`: The path to the directory containing encrypted files, mounted at `/encrypted/` with a specified key.

- `{{ untrusted_path }}`: The path to the directory containing untrusted files, mounted at `/untrusted/`.

- `{{ enclave_size }}`: The size of the enclave, specified in bytes.

- `{{ num_threads_sgx }}`: The maximum number of threads that the SGX enclave can support.

- `{{ 'true' if env.get('EDMM', '0') == '1' else 'false' }}`: A boolean value indicating whether Enhanced Dynamic Memory Management (EDMM) is enabled, based on the `EDMM` environment variable.

# Configuration Details

- `libos.entrypoint`: Specifies the entry point executable for the application.

- `loader.log_level`: Sets the logging level for the Gramine loader.

- `loader.env.OMP_NUM_THREADS`: Configures the number of OpenMP threads via an environment variable.

- `loader.env.LD_LIBRARY_PATH`: Sets the library path for dynamic linking within the enclave.

- `loader.insecure__use_cmdline_argv`: Allows the use of command-line arguments in an insecure manner.

- `fs.mounts`: Defines the file system mounts for the enclave, including paths for libraries, executables, tmpfs, encrypted, and untrusted files.

- `fs.insecure__keys.default`: Specifies the default encryption key for accessing encrypted files.

- `sgx.debug`: Enables or disables debug mode for the SGX enclave.

- `sgx.profile.mode`: Sets the profiling mode for the SGX enclave, such as "ocall_outer".

- `sgx.enable_stats`: Enables the collection of statistics within the SGX enclave.

- `sys.enable_sigterm_injection`: Allows the injection of SIGTERM signals into the enclave.

- `sgx.enclave_size`: Specifies the size of the SGX enclave.

- `sgx.max_threads`: Sets the maximum number of threads for the SGX enclave.

- `sgx.edmm_enable`: Enables or disables Enhanced Dynamic Memory Management (EDMM) for the SGX enclave.

- `sgx.trusted_files`: Lists the files that are trusted and can be accessed securely within the enclave.

- `sgx.allowed_files`: Lists the files that are allowed to be accessed, but are not necessarily trusted.

.. collapse:: Show Code

   .. code-block:: rust

      pub const MANIFEST: &str = r#"
      libos.entrypoint = "{{ executable }}"
      loader.log_level = "{{ debug }}"
      
      loader.env.OMP_NUM_THREADS = "{{ num_threads }}"
      loader.env.LD_LIBRARY_PATH = "/lib"
      loader.insecure__use_cmdline_argv = true
      
      fs.mounts = [
        { path = "/lib", uri = "file:{{ gramine.runtimedir() }}" },
        { path = "/usr/lib", uri = "file:/usr/lib" },
        { path = "{{ arch_libdir }}", uri = "file:{{ arch_libdir }}" },
        { path = "{{ executable }}", uri = "file:{{ executable }}" },
        { type = "tmpfs", path = "{{ tmpfs_path }}" },
        { type = "encrypted", path = "/encrypted/", uri = "file:{{ encrypted_path }}/", key_name = "default" },
        { path = "/untrusted/", uri = "file:{{ untrusted_path }}/" },
      ]
      
      fs.insecure__keys.default = "ffeeddccbbaa99887766554433221100"
      
      sgx.debug = true
      sgx.profile.mode = "ocall_outer"
      sgx.enable_stats = true
      sys.enable_sigterm_injection = true
      sgx.enclave_size = "{{ enclave_size }}"
      sgx.max_threads = {{ num_threads_sgx }}
      sgx.edmm_enable = {{ 'true' if env.get('EDMM', '0') == '1' else 'false' }}
      
      sgx.trusted_files = [
        "file:{{ executable }}",
        "file:{{ gramine.runtimedir( libc ) }}/",
        "file:{{ executable_path }}/",
        "file:{{ arch_libdir }}/",
        "file:/usr/{{ arch_libdir }}/",
      ]
      
      sgx.allowed_files = [
        "file:{{ untrusted_path }}/",
      ]
      "#;

DEFAULT_PERF_EVENTS
~~~~~~~~~~~~~~~~~~~

Default performance events to be monitored.

These events are used to gather various performance metrics during the execution
of an application within the Gramine enclave. The list includes CPU cycles, cache
references, and other hardware-related events that can provide insights into the
application's behavior and performance characteristics.

# Events

- `user_time`: Time spent in user mode.
- `system_time`: Time spent in system mode.
- `duration_time`: Total duration of the event.
- `cycles`: Total CPU cycles.
- `instructions`: Number of instructions executed.
- `cache-misses`: Number of cache misses.
- `L1-dcache-loads`: L1 data cache loads.
- `L1-dcache-load-misses`: L1 data cache load misses.
- `L1-dcache-prefetches`: L1 data cache prefetches.
- `L1-icache-loads`: L1 instruction cache loads.
- `L1-icache-load-misses`: L1 instruction cache load misses.
- `dTLB-loads`: Data TLB loads.
- `dTLB-load-misses`: Data TLB load misses.
- `iTLB-loads`: Instruction TLB loads.
- `iTLB-load-misses`: Instruction TLB load misses.
- `branch-loads`: Branch loads.
- `branch-load-misses`: Branch load misses.
- `branch-instructions`: Branch instructions executed.
- `branch-misses`: Branch misses.
- `cache-references`: Cache references.
- `cpu-cycles`: CPU cycles.
- `stalled-cycles-frontend`: Cycles where the frontend is stalled.
- `page-faults`: Number of page faults.

.. collapse:: Show Code

   .. code-block:: rust

      pub const DEFAULT_PERF_EVENTS: [&str; 28] = [
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


src/profiler.rs
---------------

new
~~~

Creates a new instance of `Profiler`.

This function initializes a `Profiler` with the specified configuration parameters.
It creates the output directory if it does not exist and generates an RSA private key
for signing enclaves, storing it in the specified output directory.

# Arguments

* `num_threads` - A vector specifying the number of threads to be used for each profiling task.
* `enclave_size` - A vector specifying the sizes of the enclaves to be used for profiling.
* `output_directory` - The directory where profiling results and other output files are stored.
* `debug` - A boolean flag indicating whether debugging is enabled.
* `collector` - A `DefaultCollector` used for collecting profiling data.

# Returns

Returns a `Result` containing the initialized `Profiler` instance or an `std::io::Error`
if the output directory could not be created or the RSA private key could not be generated.

# Errors

This function will return an error if the output directory cannot be created or if
there is a failure in generating the RSA private key.

.. collapse:: Show Code

   .. code-block:: rust

        pub fn new(
          num_threads: Vec<usize>,
          enclave_size: Vec<String>,
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
            num_threads,
            enclave_size,
            debug,
            collector: Arc::new(collector),
          })
        }

Profiler
~~~~~~~~

A `Profiler` is responsible for managing the benchmarking of tasks within an SGX enclave environment.

This structure is initialized with various configuration parameters such as the number of threads,
enclave sizes, output directory, and a collector for gathering profiling data. It also manages
the creation and storage of RSA private keys used for signing the enclave.

# Fields

* `private_key_path` - The file path where the RSA private key is stored.
* `output_directory` - The directory where profiling results and other output files are stored.
* `num_threads` - A vector specifying the number of threads to be used for each profiling task.
* `enclave_size` - A vector specifying the sizes of the enclaves to be used for profiling.
* `collector` - An `Arc` wrapped `DefaultCollector` used for collecting profiling data.
* `debug` - A boolean flag indicating whether debugging is enabled.

# Methods

* `profile` - Initiates the benchmarking of a given task. This method configures the environment,
  builds and signs the enclave, and executes the task while collecting profiling data.

.. collapse:: Show Code

   .. code-block:: rust

      pub struct Profiler {
        private_key_path: PathBuf,
        output_directory: PathBuf,
        num_threads: Vec<usize>,
        enclave_size: Vec<String>,
        collector: Arc<DefaultCollector>,
        debug: bool,
      }


src/stats.rs
------------

DeepTraceEvent
~~~~~~~~~~~~~~

An event from the deep trace eBPF program.

.. collapse:: Show Code

   .. code-block:: rust

      pub struct DeepTraceEvent {
        pub ev_type: u32,
        pub timestamp: u64,
      }

from_str
~~~~~~~~

Creates a Partition from a line in `/proc/partitions`

.. collapse:: Show Code

   .. code-block:: rust

        pub fn from_str(value: &str) -> Self {
          let parts = value.split_whitespace().collect::<Vec<&str>>();
          assert_eq!(parts.len(), 4);
          let major = parts[0].parse::<u32>().unwrap();
          let minor = parts[1].parse::<u32>().unwrap();
          Self {
            name: parts[3].to_string(),
            // https://man7.org/linux/man-pages/man3/makedev.3.html
            dev: major << 20 | minor,
          }
        }

LowLevelSgxCounters
~~~~~~~~~~~~~~~~~~~

A low-level view of SGX counters.

.. collapse:: Show Code

   .. code-block:: rust

      pub struct LowLevelSgxCounters {
        pub encl_load_page: u64,
        pub encl_wb: u64,
        pub vma_access: u64,
        pub vma_fault: u64,
      }

Partition
~~~~~~~~~

Partitions are loaded from `/proc/partitions`.

.. collapse:: Show Code

   .. code-block:: rust

      pub struct Partition {
        pub name: String,
        pub dev: u32,
      }

EnergySample
~~~~~~~~~~~~

A sample of energy consumption.

.. collapse:: Show Code

   .. code-block:: rust

      pub struct EnergySample {
        pub timestamp: u128,
        pub energy_uj: u64,
      }

DiskStats
~~~~~~~~~

Disk statistics collected from the eBPF program.

.. collapse:: Show Code

   .. code-block:: rust

      pub struct DiskStats {
        pub name: String,
        pub bytes: u64,
        pub perc_random: u32,
        pub perc_seq: u32,
      }

