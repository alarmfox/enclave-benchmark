# Intel-based TEE Benchmark
This tool collects metrics to compare application performance when executed in an Intel-based TEE (SGX) using Gramine versus running on bare metal.

Full documentation is available [here](https://alarmfox.github.io/enclave-benchmark/).

## Quick Start [Ubuntu 22.04 or 24.04]

If you are using Ubuntu 22.04 or 24.04, you can set up the host by running the script in `dev/setup_host.sh` (run the script from the root directory of the project).

First, clone the repository:

```sh
git clone https://github.com/alarmfox/enclave-benchmark.git
cd enclave-benchmark
```

Make the script executable and run it.

```sh
sudo chmod +x ./dev/setup_host.sh
sudo ./dev/setup_host.sh
```

For any custom setup, follow the *bare metal* instructions [here](https://alarmfox.github.io/enclave-benchmark/installation.html#bare-metal).

Now, you can run the build command (remove `--release` for a fast but unoptimized build):
```sh
cargo build --release
```
After building, the application will be in `target/<debug|release>/enclave-benchmark`.

**(Optional)** Copy the executable somewhere else:

```sh
cp target/<debug|release>/enclave-benchmark .
```

### Running
```sh
./enclave-benchmark -h
A CLI app to run benchmarks for Gramine applications

Usage: enclave-benchmark [OPTIONS] --config <CONFIG>

Options:
  -v...                  Turn debugging information on
  -c, --config <CONFIG>  Path to configuration file
      --force            Remove previous results directory (if it exists)
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

## Workload File
The application takes a `toml` file as input and performs sequential benchmarks.

Example files are stored in the `examples` directory. Below is `examples/full.toml`:

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

A workload file has two sections:
* globals: parameters used to generate experiments, output directory, custom perf_events, debug, etc.;
* task: each task is a program to benchmark and has an executable and args;

### Variables Expansion
The `toml` file is dynamic. For example, if an application executes with a different number of threads, you can mark the parameter with the `{{ num_threads }}` placeholder. On each iteration, it will be populated with an element from `globals.num_threads` (see the `make` task in the example above).

Each task can specify a `storage_type` array (see the `writer` task in the example above). Supported storage types are:
* encrypted: Gramine encrypted directory with a hardcoded key;
* tmpfs: an in-memory filesystem similar to tmpfs, which is encrypted according to Gramine;
* untrusted: simple storage with no integrity check and no encryption;

## Development
To develop on a non SGX machine, SGX excution can be disabled by setting the environment variable `EB_SKIP_SGX` with:

```sh
export EB_SKIP_SGX=1
```

and defining the `EB_SKIP_SGX` in `src/bpf/tracer.h`.

```c
#ifndef __TRACER_H
#define __TRACER_H

#define EB_SKIP_SGX

// rest of the file...
```
