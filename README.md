# Intel based TEE benchmark
This tool collects applications metrics when excuted in an Intel based TEE (SGX) using Gramine.

Full documentation is available [here](https://alarmfox.github.io/enclave-benchmark/).

**Note**: this project needs Gramine to be compiled with `debug` or `debugoptimized` options.
For example, a build configuration would be:

```sh
meson setup build/ \
  --buildtype=debugoptimized \
  -Ddirect=enabled \ 
  -Dsgx=enabled \
  -Ddcap=enabled
```
For more information, refer to [Build Gramine from source](https://gramine.readthedocs.io/en/stable/devel/building.html).

### Running
To run the example, clone the repository and execute:

```sh
# cargo run -- -c examples/example.toml -v
```
## Usage
The application takes a `toml` file as input and performs sequentials benchmark using `perf`
saving results in `csv`.

***Warning***: currently, enclave are built using a simple manifest file and only single binary 
applications are allowed. In the future, options to specify custom manifest will be addded.

Example files are stored in the `examples` directory. Below `examples/full.toml`:

```toml
[globals]
sample_size = 3
epc_size = ["64M", "128M"]
output_directory = "/tmp/test"
num_threads = [2, 4]
extra_perf_events = ["cpu-clock"]
energy_sample_interval = "100ms"

[[tasks]]
pre_run_executable = "/usr/bin/echo"
pre_run_args = ["Before task"]

executable = "/bin/dd"
args = ["if=/dev/zero", "of=/dev/null", "count=10000"]

post_run_executable = "/usr/bin/echo"
post_run_args = ["After task"]

[[tasks]]
executable = "/usr/bin/make"
args = ["-C", "examples/basic-c-app/", "-j", "{{ num_threads }}", "clean", "app"]

[[tasks]]
executable = "./examples/simple-writer/writer"
args = ["{{ output_directory }}"]
storage_type = ["encrypted", "tmpfs", "trusted", "untrusted"]

```
A workload file has 2 sections:
* globals: parameters used to generate experiments, output directory and add custom perf_events;
* task: each task is a program to benchmark and has an executable and args;

### Variables expansion
The `toml` file is dynamic. For example, if an application executes with different number of threads you can mark the parameter with the `{{ num_threads }}` placeholder. On each iteration it will be populated with an element from `globals.num_threads` (see `make` task in the example above).

Each task can specify a `storage_type` array (see `writer` task in the example above). Supported storage are:
* encrypted: Gramine encrypted directory with an hardcoded key;
* tmpfs: an in memory filesystem similar to tmpfs which is encrypted according to Gramine;
* trusted: storage with integrity check and `chroot` environment;
* untrusted: simple storage with no integrity check and no encryption;

Results will be stored in `output_directory` and it will have the following structure (obtained executing `examples/minimal.toml`):

```sh
tree -L 6 /tmp/test

/tmp/test/
|-- private_key.pem
`-- sleep
    |-- gramine-sgx
    |   |-- sleep-2-64M
    |   |   |-- encrypted
    |   |   |-- sleep-2-64M-untrusted
    |   |   |   |-- 1
    |   |   |   |   |-- package-0-core.csv
    |   |   |   |   |-- package-0-dram.csv
    |   |   |   |   |-- package-0-uncore.csv
    |   |   |   |   |-- package-0.csv
    |   |   |   |   |-- perf.csv
    |   |   |   |-- 2
    |   |   |   |   |-- package-0-core.csv
    |   |   |   |   |-- package-0-dram.csv
    |   |   |   |   |-- package-0-uncore.csv
    |   |   |   |   |-- package-0.csv
    |   |   |   |   |-- perf.csv
    |   |   |   `-- 3
    |   |   |       |-- package-0-core.csv
    |   |   |       |-- package-0-dram.csv
    |   |   |       |-- package-0-uncore.csv
    |   |   |       |-- package-0.csv
    |   |   |       |-- perf.csv
    |   |   |-- sleep.manifest.sgx
    |   |   |-- sleep.sig
    |   |   |-- trusted
    |   |   `-- untrusted
    |   `-- sleep-4-64M
    |       |-- encrypted
    |       |-- sleep-4-64M-untrusted
    |       |   |-- 1
    |       |   |   |-- package-0-core.csv
    |       |   |   |-- package-0-dram.csv
    |       |   |   |-- package-0-uncore.csv
    |       |   |   |-- package-0.csv
    |       |   |   |-- perf.csv
    |       |   |-- 2
    |       |   |   |-- package-0-core.csv
    |       |   |   |-- package-0-dram.csv
    |       |   |   |-- package-0-uncore.csv
    |       |   |   |-- package-0.csv
    |       |   |   |-- perf.csv
    |       |   `-- 3
    |       |       |-- package-0-core.csv
    |       |       |-- package-0-dram.csv
    |       |       |-- package-0-uncore.csv
    |       |       |-- package-0.csv
    |       |       |-- perf.csv
    |       |-- sleep.manifest.sgx
    |       |-- sleep.sig
    |       |-- trusted
    |       `-- untrusted
    `-- no-gramine-sgx
        |-- sleep-2
        |   |-- 1
        |   |   |-- package-0-core.csv
        |   |   |-- package-0-dram.csv
        |   |   |-- package-0-uncore.csv
        |   |   |-- package-0.csv
        |   |   |-- perf.csv
        |   |-- 2
        |   |   |-- package-0-core.csv
        |   |   |-- package-0-dram.csv
        |   |   |-- package-0-uncore.csv
        |   |   |-- package-0.csv
        |   |   |-- perf.csv
        |   |-- 3
        |   |   |-- package-0-core.csv
        |   |   |-- package-0-dram.csv
        |   |   |-- package-0-uncore.csv
        |   |   |-- package-0.csv
        |   |   |-- perf.csv
        |   `-- storage
        `-- sleep-4
            |-- 1
            |   |-- package-0-core.csv
            |   |-- package-0-dram.csv
            |   |-- package-0-uncore.csv
            |   |-- package-0.csv
            |   |-- perf.csv
            |-- 2
            |   |-- package-0-core.csv
            |   |-- package-0-dram.csv
            |   |-- package-0-uncore.csv
            |   |-- package-0.csv
            |   |-- perf.csv
            |-- 3
            |   |-- package-0-core.csv
            |   |-- package-0-dram.csv
            |   |-- package-0-uncore.csv
            |   |-- package-0.csv
            |   |-- perf.csv
            `-- storage
```

## Python bindings
This projects uses [Gramine Python API](https://gramine.readthedocs.io/en/stable/python/api.html) 
with [PyO3](https://github.com/PyO3/pyo3) and needs `python3-dev[evel]` package. You will need 
to install it according to your distribution. For example, on Ubuntu:

```sh
# apt-get install python3-dev
```
