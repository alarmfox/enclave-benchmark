# Intel based TEE benchmark
This tool collects metrics to compare application performance when executed in an Intel-based TEE (SGX) 
using Gramine versus running on bare metal.

Full documentation is available [here](https://alarmfox.github.io/enclave-benchmark/).

## Quick start [Ubuntu 22.04 or 24.04]
If on Ubunt 22.04 or 24.04, the host can be setup by running the script in `dev/setup_host.sh`

```sh
sudo ./dev/setup_host.sh
```

Now, you can run the build command (remove `--release` for a fast but unoptimized build):
```sh
cargo build --release
```
After building the application will be in `target/<debug|release>/enclave-benchmark`.

**(Optional)** Copy the executable somewhere else:

```sh
cp target/<debug|release>/enclave-benchmark .
```

### Running
```sh
./enclave-benchmark -h
A cli app to run benchmarks for Gramine application

Usage: enclave-benchmark [OPTIONS] --config <CONFIG>

Options:
  -v...                  Turn debugging information on
  -c, --config <CONFIG>  Path to configuration file
      --force            Remove previous results directory (if exists)
  -h, --help             Print help
  -V, --version          Print version
```

Run an example workload with:
```sh
sudo ./enclave-benchmark -v -c examples/iobound.toml
```

The `test` directory will look like:

```sh
tree test

test
├── dd
│   ├── gramine-sgx
│   │   └── dd-1-256M
│   │       ├── dd-1-256M-encrypted
│   │       │   └── 1
│   │       │       ├── io.csv
│   │       │       ├── package-0-core.csv
│   │       │       ├── package-0.csv
│   │       │       ├── package-0-dram.csv
│   │       │       ├── package-0-uncore.csv
│   │       │       ├── perf.csv
│   │       │       ├── stderr
│   │       │       └── stdout
│   │       ├── dd-1-256M-tmpfs
│   │       │   └── 1
│   │       │       ├── io.csv
│   │       │       ├── package-0-core.csv
│   │       │       ├── package-0.csv
│   │       │       ├── package-0-dram.csv
│   │       │       ├── package-0-uncore.csv
│   │       │       ├── perf.csv
│   │       │       ├── stderr
│   │       │       └── stdout
│   │       ├── dd-1-256M-untrusted
│   │       │   └── 1
│   │       │       ├── io.csv
│   │       │       ├── package-0-core.csv
│   │       │       ├── package-0.csv
│   │       │       ├── package-0-dram.csv
│   │       │       ├── package-0-uncore.csv
│   │       │       ├── perf.csv
│   │       │       ├── stderr
│   │       │       └── stdout
│   │       ├── dd.manifest.sgx
│   │       ├── dd.sig
│   │       ├── encrypted
│   │       │   └── a.zero
│   │       └── untrusted
│   │           └── a.zero
│   └── no-gramine-sgx
│       └── dd-1
│           ├── 1
│           │   ├── io.csv
│           │   ├── package-0-core.csv
│           │   ├── package-0.csv
│           │   ├── package-0-dram.csv
│           │   ├── package-0-uncore.csv
│           │   ├── perf.csv
│           │   ├── stderr
│           │   └── stdout
│           └── storage
│               └── a.zero
└── private_key.pem
```

## Workload file
The application takes a `toml` file as input and performs sequentials benchmark. 

Example files are stored in the `examples` directory. Below `examples/full.toml`:

```toml
[globals]
sample_size = 3
epc_size = ["64M", "128M"]
output_directory = "/tmp/test"
num_threads = [2, 4]
extra_perf_events = ["cpu-clock"]
energy_sample_interval = "100ms"
debug = true

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
storage_type = ["encrypted", "tmpfs", "untrusted"]

```
A workload file has 2 sections:
* globals: parameters used to generate experiments, output directory, custom perf_events, debug etc.;
* task: each task is a program to benchmark and has an executable and args;

### Variables expansion
The `toml` file is dynamic. For example, if an application executes with different number of threads
you can mark the parameter with the `{{ num_threads }}` placeholder. On each iteration it will be
populated with an element from `globals.num_threads` (see `make` task in the example above).

Each task can specify a `storage_type` array (see `writer` task in the example above). Supported storage are:
* encrypted: Gramine encrypted directory with an hardcoded key;
* tmpfs: an in memory filesystem similar to tmpfs which is encrypted according to Gramine;
* untrusted: simple storage with no integrity check and no encryption;
