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
```
A workload file has 2 sections:
* globals: parameters used to generate experiments, output directory and add custom perf_events;
* task: each task is a program to benchmark and has an executable and args;

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
`-- private_key.pem
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
