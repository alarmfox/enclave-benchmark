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

## Usage
The application takes a `toml` file as input and performs sequentials benchmark using `perf`
saving results in `csv`.

***Warning***: currently, enclave are built using a simple manifest file and only single binary 
applications are allowed. In the future, options to specify custom manifest will be addded.

An example file is stored in `examples/basic.toml`.

```toml
[globals]
sample_size = 10
epc_size = ["64M", "128M"]
output_directory = "/tmp/test"
num_threads = [1, 2, 4]
extra_perf_events = ["cpu-clock"]

[[tasks]]
executable = "/bin/ls"
args = ["-l", "-a"]

[[tasks]]
executable = "/bin/dd"
args = ["if=/dev/zero", "of=/dev/null", "count=10000"]

[[tasks]]
executable = "/usr/bin/make"
args = ["-C", "/path/to/some/project", "-j", "{{ num_threads }}"]
```
A workload file has 2 sections:
* globals: parameters used to generate experiments, output directory and add custom perf_events;
* task: each task is a program to benchmark and has an executable and args;

### Variables expansion
The `toml` file is dynamic. For example, if an application executes with different number of threads you can mark the parameter with the `{{ num_threads }}` placeholder. On each iteration it will be populated with an element from `globals.num_threads` (see `make` task in the example above).

Results will be stored in `output_directory` and it will have the following structure:

```sh
# tree -L 2 /tmp/test

/tmp/test/
|-- dd
|   |-- dd-1.no_sgx.csv
|   |-- dd-2.no_sgx.csv
|   |-- dd-4.no_sgx.csv
|   |-- dd.manifest.sgx
|   `-- dd.sig
|-- ls
|   |-- ls-1.no_sgx.csv
|   |-- ls-2.no_sgx.csv
|   |-- ls-4.no_sgx.csv
|   |-- ls.manifest.sgx
|   `-- ls.sig
|-- make
|   |-- make-1.no_sgx.csv
|   |-- make-2.no_sgx.csv
|   |-- make-4.no_sgx.csv
|   |-- make.manifest.sgx
|   `-- make.sig
`-- private_key.pem

```

### Running
To run the example, clone the repository and:

```sh
# cargo run -- -c examples/basic.toml -v
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
