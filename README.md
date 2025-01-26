# Intel based TEE benchmark
This tool collects applications metrics when excuted in an Intel based TEE (SGX) using Gramine.

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

[[tasks]]
executable = "/bin/dd"
args = ["if=/dev/zero", "of=/dev/null", "count=10000"]

[[tasks]]
executable = "/usr/bin/make"
args = ["-C", "examples/basic-c-app/", "-j", "{{ num_threads }}", "clean", "app"]

[[tasks]]
executable = "./examples/simple-writer/writer"
args = ["{{ output_directory }}"]
storage_type = ["encrypted", "plaintext", "tmpfs", "trusted"]

```
A workload file has 2 sections:
* globals: parameters used to generate experiments, output directory and add custom perf_events;
* task: each task is a program to benchmark and has an executable and args;

### Variables expansion
The `toml` file is dynamic. For example, if an application executes with different number of threads you can mark the parameter with the `{{ num_threads }}` placeholder. On each iteration it will be populated with an element from `globals.num_threads` (see `make` task in the example above).

Each task can specify a `storage_type` array (see `writer` task in the example above). Supported storage are:
* encrypted: Gramine encrypted directory with an hardcoded key;
* plaintext: simple storage with no integrity check and no encryption;
* tmpfs: an in memory filesystem similar to tmpfs which is encrypted according to Gramine;
* trusted: storage with integrity check and `chroot` environment;

Results will be stored in `output_directory` and it will have the following structure (output reported only for task **dd**):

```sh
# tree -L 6 /tmp/test

/tmp/test/
|-- dd
|   |-- gramine-sgx
|   |   |-- dd-2-64M
|   |   |   |-- dd-2-64M-plaintext
|   |   |   |   |-- 1
|   |   |   |   |   |-- perf.csv
|   |   |   |   |   `-- strace.log
|   |   |   |   |-- 2
|   |   |   |   |   |-- perf.csv
|   |   |   |   |   `-- strace.log
|   |   |   |   `-- 3
|   |   |   |       |-- perf.csv
|   |   |   |       `-- strace.log
|   |   |   |-- dd.manifest.sgx
|   |   |   |-- dd.sig
|   |   |   |-- encrypted
|   |   |   |-- plaintext
|   |   |   `-- trusted
|   |   `-- dd-4-64M
|   |       |-- dd-4-64M-plaintext
|   |       |   |-- 1
|   |       |   |   |-- perf.csv
|   |       |   |   `-- strace.log
|   |       |   |-- 2
|   |       |   |   |-- perf.csv
|   |       |   |   `-- strace.log
|   |       |   `-- 3
|   |       |       |-- perf.csv
|   |       |       `-- strace.log
|   |       |-- dd.manifest.sgx
|   |       |-- dd.sig
|   |       |-- encrypted
|   |       |-- plaintext
|   |       `-- trusted
|   `-- no-gramine-sgx
|       |-- dd-2
|       |   |-- 1
|       |   |   |-- perf.csv
|       |   |   `-- strace.log
|       |   |-- 2
|       |   |   |-- perf.csv
|       |   |   `-- strace.log
|       |   |-- 3
|       |   |   |-- perf.csv
|       |   |   `-- strace.log
|       |   `-- storage
|       `-- dd-4
|           |-- 1
|           |   |-- perf.csv
|           |   `-- strace.log
|           |-- 2
|           |   |-- perf.csv
|           |   `-- strace.log
|           |-- 3
|           |   |-- perf.csv
|           |   `-- strace.log
|           `-- storage
|-- private_key.pem
```
## Python bindings
This projects uses [Gramine Python API](https://gramine.readthedocs.io/en/stable/python/api.html) 
with [PyO3](https://github.com/PyO3/pyo3) and needs `python3-dev[evel]` package. You will need 
to install it according to your distribution. For example, on Ubuntu:

```sh
# apt-get install python3-dev
```

### Perf considerations
This tool heavily rely on [Perf](https://perfwiki.github.io/main/) which requires `sudo` permission. 

To avoid the hassle of using `sudo`, you can use:

```sh
# sh -c 'echo 1 > /proc/sys/kernel/perf_event_paranoid'
```

If using Docker, run the container with `--privileged` flag.
