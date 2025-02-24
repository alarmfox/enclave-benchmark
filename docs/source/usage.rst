Usage
=====

The application takes a `toml` file as input and performs sequentials benchmark. 

Example files are stored in the `examples` directory. Below `examples/full.toml`:

.. code:: toml

  [globals]
  sample_size = 1
  output_directory = "demo-result"
  extra_perf_events = ["cpu-clock"]
  debug = false
  deep_trace = true

  [[tasks]]
  executable = "/usr/bin/sysbench"
  args = ["--threads={{ num_threads }}", "cpu", "run"]
  enclave_size = ["1G"]
  num_threads = [1, 2, 4]
  env = { OMP_NUM_THREADS = "{{ num_threads }}" }

  [[tasks]]
  executable = "/bin/dd"
  args = ["if=/dev/random", "of={{ output_directory }}/a.random", "count=1000000"]
  storage_type = ["encrypted", "untrusted"]
  enclave_size = ["256M", "512M"]

  post_run_executable = "rm"
  post_run_args = ["-rf", "{{ output_directory }}/a.random"]


The application needs to be run always with **root** privileges.

.. code:: sh

  ./enclave-benchmark -h 

  A cli app to run benchmarks for Gramine application

  Usage: enclave-benchmark [OPTIONS] --config <CONFIG>

  Options:
    -v...                  Turn debugging information on
    -c, --config <CONFIG>  Path to configuration file
        --force            Remove previous results directory (if exists)
        --aggregate        Aggregate results from samples. Creates an <output_directory>/aggregated
    -h, --help             Print help
    -V, --version          Print version

Input File Specification
------------------------

This section describes the format and meaning of each field in the input file used for benchmarking Gramine applications.

Global Configuration
^^^^^^^^^^^^^^^^^^^^

The `[globals]` section defines settings that apply to all benchmark runs.

- **sample_size** (integer)  
  Specifies the number of times each experiment is repeated.

- **output_directory** (string)  
  The directory where benchmark results and outputs are stored. This variable can be referenced in task configurations using `{{ output_directory }}`.

- **extra_perf_events** (list of strings)  
  Specifies additional performance monitoring events to be collected.  
  Example: `["cpu-clock"]` enables CPU cycle counting.

- **energy_sample_interval** (string)  
  Defines the interval at which energy consumption is sampled. Must be specified with a time unit (e.g., `"250ms"` for 250 milliseconds).

- **debug** (boolean)  
  If `true`, enables debug logging for more detailed output.

- **deep_trace** (boolean)  
  If `true`, enables an extra experiment with tracing enabled. This can be very slow.


Tasks
"""""

Each `[[tasks]]` section defines a specific executable or command to run in the benchmark.

- **executable** (string)  
  The path to the executable that is benchmarked.  
  Example: `"/bin/dd"`

- **args** (list of strings)  
  Command-line arguments passed to the executable.  
  Example: `["if=/dev/zero", "of=/dev/null", "count=10000"]` runs `dd` with these arguments.

- **enclave_size** (list of strings)  
  Defines the possible enclave memory sizes. Each experiment will be run with every listed size.  
  Example: `["64M", "128M"]` runs experiments with enclaves of `64MB` and `128MB`.

- **num_threads** (list of integers)  
  Specifies the number of threads to be used in each experiment. The application expands `{{ num_threads }}` for every experiment.
  Default: `1`.

Optional Task Fields
^^^^^^^^^^^^^^^^^^^^

Some tasks include additional fields:

- **env** (map)
  Specifies environment variables for the target process. Values are also expanded as arguments.
  Example: `env = { OMP_NUM_THREADS = "{{ num_threads }}"}`

- **pre_run_executable** (string)  
  An executable to run before the main task.  
  Example: `"/usr/bin/echo"`.

- **pre_run_args** (list of strings)  
  Arguments for the `pre_run_executable`.  
  Example: `["Starting make"]`.

- **post_run_executable** (string)  
  An executable to run after the main task.  
  Example: `"/usr/bin/make"`.

- **post_run_args** (list of strings)  
  Arguments for the `post_run_executable`.  
  Example: `["-C", "examples/basic-c-app", "clean", "output={{ output_directory }}"]`.

- **storage_type** (list of strings)  
  Specifies different storage modes to be tested.  
  Example: `["encrypted", "tmpfs", "untrusted"]` runs experiments under each of these storage types.

Variable Expansion
^^^^^^^^^^^^^^^^^^
Some fields contain **placeholders** that are expanded dynamically for each experiment:

- `{{ output_directory }}`  
  Expands to the value of the directory mounted for relevant app storage. In Gramine applications, storage can be encrypted or untrusted.

- `{{ num_threads }}`  
  Expands to each value in `num_threads` during benchmarking.

