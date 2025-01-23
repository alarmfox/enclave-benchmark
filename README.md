# Intel based TEE benchmark
This tool collects applications metrics when excuted in an Intel based TEE (SGX) using Gramine.

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
num_threads = [1]
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

### Perf considerations
This tool heavily rely on [Perf](https://perfwiki.github.io/main/) which requires `sudo` permission. 

To avoid the hassle of using `sudo`, you can use:

```sh
# sh -c 'echo 1 > /proc/sys/kernel/perf_event_paranoid'
```

If using Docker, run the container with `--privileged` flag.
