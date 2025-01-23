# Intel based TEE benchmark
This tool collects applications metrics when excuted in an Intel based TEE (SGX) using Gramine.

## Perf 
This tool heavily rely on Perf, which requires `sudo` permission. 

To avoid the hassle of using `sudo`, you can use:

```sh
# sh -c 'echo 1 > /proc/sys/kernel/perf_event_paranoid'
```

If using Docker, run the container with `--privileged` flag.
