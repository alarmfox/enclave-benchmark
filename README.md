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

#### Troubleshooting
If you encounter the following error:
```sh
libbpf: failed to determine tracepoint 'syscalls/sys_enter_read' perf event ID: No such file or directory
libbpf: prog 'trace_enter_read': failed to create tracepoint 'syscalls/sys_enter_read' perf event: No such file or directory
libbpf: prog 'trace_enter_read': failed to auto-attach: -2

```
It indicates that `debugfs` is not mounted. You can mount it with the following command:
```sh
sudo mount -t debugfs none /sys/kernel/debug
```

#### Output directory
The `test` directory will look like:

```sh
cd test
tree .

├── dd
│   ├── gramine-sgx
│   │   └── dd-1-256M
│   │       ├── dd-1-256M-encrypted
│   │       │   ├── 1
│   │       │   │   ├── io.csv
│   │       │   │   ├── package-0-core.csv
│   │       │   │   ├── package-0.csv
│   │       │   │   ├── package-0-dram.csv
│   │       │   │   ├── package-0-uncore.csv
│   │       │   │   ├── perf.csv
│   │       │   │   ├── stderr
│   │       │   │   └── stdout
│   │       │   └── deep-trace
│   │       │       ├── io.csv
│   │       │       ├── package-0-core.csv
│   │       │       ├── package-0.csv
│   │       │       ├── package-0-dram.csv
│   │       │       ├── package-0-uncore.csv
│   │       │       ├── perf.csv
│   │       │       ├── stderr
│   │       │       ├── stdout
│   │       │       └── trace.csv
│   │       ├── dd-1-256M-tmpfs
│   │       │   ├── 1
│   │       │   │   ├── io.csv
│   │       │   │   ├── package-0-core.csv
│   │       │   │   ├── package-0.csv
│   │       │   │   ├── package-0-dram.csv
│   │       │   │   ├── package-0-uncore.csv
│   │       │   │   ├── perf.csv
│   │       │   │   ├── stderr
│   │       │   │   └── stdout
│   │       │   └── deep-trace
│   │       │       ├── io.csv
│   │       │       ├── package-0-core.csv
│   │       │       ├── package-0.csv
│   │       │       ├── package-0-dram.csv
│   │       │       ├── package-0-uncore.csv
│   │       │       ├── perf.csv
│   │       │       ├── stderr
│   │       │       ├── stdout
│   │       │       └── trace.csv
│   │       ├── dd-1-256M-untrusted
│   │       │   ├── 1
│   │       │   │   ├── io.csv
│   │       │   │   ├── package-0-core.csv
│   │       │   │   ├── package-0.csv
│   │       │   │   ├── package-0-dram.csv
│   │       │   │   ├── package-0-uncore.csv
│   │       │   │   ├── perf.csv
│   │       │   │   ├── stderr
│   │       │   │   └── stdout
│   │       │   └── deep-trace
│   │       │       ├── io.csv
│   │       │       ├── package-0-core.csv
│   │       │       ├── package-0.csv
│   │       │       ├── package-0-dram.csv
│   │       │       ├── package-0-uncore.csv
│   │       │       ├── perf.csv
│   │       │       ├── stderr
│   │       │       ├── stdout
│   │       │       └── trace.csv
│   │       ├── dd.manifest.sgx
│   │       ├── dd.sig
│   │       ├── encrypted
│   │       │   └── a.zero
│   │       └── untrusted
│   │           └── a.zero
│   └── no-gramine-sgx
│       └── dd-1
│           ├── dd-1-untrusted
│           │   ├── 1
│           │   │   ├── io.csv
│           │   │   ├── package-0-core.csv
│           │   │   ├── package-0.csv
│           │   │   ├── package-0-dram.csv
│           │   │   ├── package-0-uncore.csv
│           │   │   ├── perf.csv
│           │   │   ├── stderr
│           │   │   └── stdout
│           │   └── deep-trace
│           │       ├── io.csv
│           │       ├── package-0-core.csv
│           │       ├── package-0.csv
│           │       ├── package-0-dram.csv
│           │       ├── package-0-uncore.csv
│           │       ├── perf.csv
│           │       ├── stderr
│           │       ├── stdout
│           │       └── trace.csv
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
deep_trace = true

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

### Deep trace
When `deep_trace = true`, the application does an extra excution collecting events such as:
* kernel memory allocation;
* disk access;
* system read/write operations;

Precise events are:
* "sys-read",
* "sys-write",
* "mm-page-alloc",
* "mm-page-free",
* "kmalloc",
* "kfree",
* "disk-read",
* "disk-write",

Each event has a unique timestamp and results are collected in `trace.csv`.

```
timestamp (ns),event
1402564790125152,sys-read
1402564790127602,sys-read
1402564790128530,sys-write
1402564790130190,mem-read
1402564790130191,mem-write
1402564790130982,disk-read
1402564790131739,disk-read
1402564790132306,mm-page-alloc
1402564790132574,mm-page-free
1402564790133426,kmalloc
1402564790133545,kfree
```

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
