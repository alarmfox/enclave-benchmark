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

pub const ENERGY_CSV_HEADER: &str = "timestamp (microseconds),energy (microjoule)";
pub const IO_CSV_HEADER: &str = "dimension,unit,value,description";
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
