/// The Gramine manifest configuration for an enclave application.
///
/// This manifest defines various settings and parameters for running an application
/// within a Gramine enclave. It includes configuration for the entry point, logging,
/// environment variables, file system mounts, security settings, and SGX-specific options.
///
/// # Variables
///
/// - `{{ executable }}`: The path to the executable that serves as the entry point for the application.
///
/// - `{{ debug }}`: The log level for the loader, which determines the verbosity of logging output.
///
/// - `{{ env }}`: Environment variables for the application.
///
/// - `{{ num_threads }}`: The number of OpenMP threads to be used by the application, set via the `OMP_NUM_THREADS` environment variable.
///
/// - `{{ gramine.runtimedir() }}`: The directory path where Gramine runtime libraries are located, used for mounting the `/lib` path.
///
/// - `{{ arch_libdir }}`: The architecture-specific library directory, used for mounting and trusted file paths.
///
/// - `{{ encrypted_path }}`: The path to the directory containing encrypted files, mounted at `/encrypted/` with a specified key.
///
/// - `{{ untrusted_path }}`: The path to the directory containing untrusted files, mounted at `/untrusted/`.
///
/// - `{{ enclave_size }}`: The size of the enclave, specified in bytes.
///
/// - `{{ num_threads_sgx }}`: The maximum number of threads that the SGX enclave can support.
///
/// - `{{ 'true' if env.get('EDMM', '0') == '1' else 'false' }}`: A boolean value indicating whether Enhanced Dynamic Memory Management (EDMM) is enabled, based on the `EDMM` environment variable.
///
/// # Configuration Details
///
/// - `libos.entrypoint`: Specifies the entry point executable for the application.
///
/// - `loader.log_level`: Sets the logging level for the Gramine loader.
///
/// - `loader.env.LD_LIBRARY_PATH`: Sets the library path for dynamic linking within the enclave.
///
/// - `loader.insecure__use_cmdline_argv`: Allows the use of command-line arguments in an insecure manner.
///
/// - `fs.mounts`: Defines the file system mounts for the enclave, including paths for libraries, executables, tmpfs, encrypted, and untrusted files.
///
/// - `fs.insecure__keys.default`: Specifies the default encryption key for accessing encrypted files.
///
/// - `sgx.debug`: Enables or disables debug mode for the SGX enclave.
///
/// - `sgx.profile.mode`: Sets the profiling mode for the SGX enclave, such as "ocall_outer".
///
/// - `sgx.enable_stats`: Enables the collection of statistics within the SGX enclave.
///
/// - `sys.enable_sigterm_injection`: Allows the injection of SIGTERM signals into the enclave.
///
/// - `sgx.enclave_size`: Specifies the size of the SGX enclave.
///
/// - `sgx.max_threads`: Sets the maximum number of threads for the SGX enclave.
///
/// - `sgx.edmm_enable`: Enables or disables Enhanced Dynamic Memory Management (EDMM) for the SGX enclave.
///
/// - `sgx.trusted_files`: Lists the files that are trusted and can be accessed securely within the enclave.
///
/// - `sgx.allowed_files`: Lists the files that are allowed to be accessed, but are not necessarily trusted.
pub const MANIFEST: &str = r#"
libos.entrypoint = "{{ executable }}"
loader.log_level = "{{ debug }}"

loader.env.LD_LIBRARY_PATH = "/lib:{{ arch_libdir }}:/usr/lib"
loader.insecure__use_cmdline_argv = true

{% for key, val in env.items() %}
    loader.env.{{ key }} = "{{ val }}"
{% endfor %}

fs.mounts = [
  { path = "/lib", uri = "file:{{ gramine.runtimedir() }}" },
  { path = "/usr/lib", uri = "file:/usr/lib" },
  { path = "{{ arch_libdir }}", uri = "file:{{ arch_libdir }}" },
  { path = "{{ executable }}", uri = "file:{{ executable }}" },
  { type = "tmpfs", path = "/tmp/" },
  { type = "encrypted", path = "/encrypted/", uri = "file:{{ encrypted_path }}/", key_name = "default" },
  { path = "/untrusted/", uri = "file:{{ untrusted_path }}/" },
  { path = "/etc/passwd", uri = "file:/etc/passwd" }
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
  "file:/etc/passwd"
]

sgx.allowed_files = [
  "file:{{ untrusted_path }}/",
]
"#;

pub const ENERGY_CSV_HEADER: &str = "timestamp (ns),energy (microjoule)";
pub const IO_CSV_HEADER: &str = "dimension,unit,value,description";
pub const TRACE_CSV_HEADER: &str = "timestamp (ns),event";

/// Default performance events to be monitored.
///
/// These events are used to gather various performance metrics during the execution
/// of an application within the Gramine enclave. The list includes CPU cycles, cache
/// references, and other hardware-related events that can provide insights into the
/// application's behavior and performance characteristics.
///
/// # Events
///
/// - `branch-misses`: Number of branch misses.
/// - `cache-misses`: Number of cache misses.
/// - `cpu-cycles`: CPU cycles.
/// - `duration_time`: Total duration of the event.
/// - `instructions`: Number of instructions executed.
/// - `stalled-cycles-frontend`: Cycles where the frontend is stalled.
/// - `system_time`: Time spent in system mode.
/// - `user_time`: Time spent in user mode.
pub const DEFAULT_PERF_EVENTS: [&str; 8] = [
  "branch-misses",
  "cache-misses",
  "cpu-cycles",
  "duration_time",
  "instructions",
  "stalled-cycles-frontend",
  "system_time",
  "user_time",
];
