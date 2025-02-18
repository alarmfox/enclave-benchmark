# Enclave-Benchmark
This tool collects metrics to compare application performance when executed in an Intel-based TEE (SGX) using [Gramine](https://gramine.readthedocs.io/en/stable/) versus running on bare metal.

Full documentation is available [here](https://alarmfox.github.io/enclave-benchmark/).

## Quick Start (Ubuntu 22.04 or 24.04)

If you are using Ubuntu 22.04 or 24.04, you can set up the host by running the script in `dev/setup_host.sh` (run the script from the root directory of the project).

First, get the source code. If you don't want to contribute use the [latest release](https://github.com/alarmfox/enclave-benchmark/archive/refs/tags/v0.1.0.tar.gz) and extraxct it somewhere with:

```sh
tar xvf </path/to/tar>
cd enclave-benchmark-v0.1.0
```

Otherwise, clone the repository:

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

#### Development

If you want to develop, use the script `dev/setup_env.sh`:

```sh
source dev/setup_env.sh
```

This script will define the `EB_SKIP_SGX` in `src/bpf/tracer.h` and it will export the environment variable `EB_SKIP_SGX=1`.
This is needed if you develop on a non-sgx machine.

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

## Analyzing results
### Preprocessing for Data Aggregation

To aggregate sample data with mean and standard deviation, a preprocessing step is required. This is accomplished using the script `dev/pre-process.py`. The script processes the raw data collected during benchmarking and computes the necessary statistical measures for analysis.

#### Setting Up the Environment

Before running the preprocessing script, ensure that you have a suitable Python environment. You can create a virtual environment and install the necessary dependencies using the `requirements.txt` file. This ensures that all required packages are available for the script to run smoothly.

```sh
python3 -m venv env
source env/bin/activate
pip install -r requirements.txt
````

#### Running the Preprocessing Script

The `dev/pre-process.py` script requires two arguments to function correctly:

1. **Path to the TOML File**: This is the configuration file used during the benchmarking process. It contains the parameters and tasks that were executed.

2. **Output Directory**: This is the directory where the aggregated data will be stored after processing.

To run the script, use the following command:

```sh
python dev/pre-process.py <path/to/toml> <path/to/output_directory>
```

For example, running the following commands produces:
```sh
python dev/pre-process.py examples/demo.toml /tmp/demo-processed
tree /tmp/demo-processed/

/tmp/demo-processed/
├── bonnie++-1-untrusted
│   ├── package-0-core.csv
│   ├── package-0.csv
│   └── perf.csv
├── bonnie++-2-untrusted
│   ├── package-0-core.csv
│   ├── package-0.csv
│   └── perf.csv
├── bonnie++-4-untrusted
│   ├── package-0-core.csv
│   ├── package-0.csv
│   └── perf.csv
├── launch_nbody.sh-1-untrusted
│   ├── package-0-core.csv
│   ├── package-0.csv
│   └── perf.csv
├── launch_nbody.sh-2-untrusted
│   ├── package-0-core.csv
│   ├── package-0.csv
│   └── perf.csv
└── launch_nbody.sh-4-untrusted
    ├── package-0-core.csv
    ├── package-0.csv
    └── perf.csv
```

#### Perf Aggregation
Perf output is aggregated by calculating the mean and standard deviation for each counter. The output appears as follows.

```sh
cat /tmp/demo-processed/bonnie++-1-untrusted/perf.csv

event,counter_mean,counter_std,counter_unit,metric_mean,unit_metric,perc_runtime_mean
L1-dcache-load-misses,5113360521.6,103267837.73247407,,3.4019999999999997,of all L1-dcache accesses,31.0
L1-dcache-loads,150305474853.2,205182282.16844067,,1.1228,G/sec,31.0
L1-dcache-prefetches,1269258560.2,52122983.51175186,,9.4724,M/sec,31.0
L1-icache-load-misses,2304340677.4,19231482.942564532,,1.078,of all L1-icache accesses,31.0
L1-icache-loads,214014918517.2,261940713.8728049,,1.5986,G/sec,31.0
branch-instructions,80574228395.8,171678771.61877853,,601.847,M/sec,31.0
branch-load-misses,9837754039.2,26359858.744911663,,73.4822,M/sec,31.0
branch-loads,80564398959.6,163850241.16953686,,601.7755999999999,M/sec,31.0
branch-misses,9836256318.8,24316591.9320201,,12.206,of all branches,31.0
....
```

#### Energy Measurement Aggregation

Energy measurements are aggregated using the **coalescing window method**. This involves grouping energy samples into fixed time intervals, or **windows**, to align them with other data samples. By default, the window size (`W`) is set to `100ms`. Within each window, energy samples are averaged to compute the mean and standard deviation. This ensures energy data is accurately represented over consistent time intervals, allowing for meaningful comparisons with other metrics collected during benchmarking.


```sh
head  /tmp/demo-processed/launch_nbody.sh-1-untrusted/package-0.csv

,bin,relative_time,energy (microjoule)
0,0,0.0,20926838658.2
1,50018408,500184086.0,22027975500.0
2,50019136,500191369.0,21457430207.0
3,50023813,500238137.0,19852073423.0
4,50026094,500260940.0,20929957912.0
5,50031162,500311624.0,20389345794.0
6,100040329,1000403297.0,21462625388.0
7,100047796,1000477967.0,19856279871.0
8,100049695,1000496953.0,22029620099.0
```
