How does it work?
=================

For each task, the application builds a series of experiments based on `globals.num_threads`,
`globals.enclave_size`, and `task.storage` (for Gramine runs only).

The application launches the target program in a separate process and starts the 
monitoring in 3 threads:

- eBPF collector: runs eBPF programs and collects low-level stats;
- energy monitor: polls energy consumption;
- performance counters: runs perf in a separate process;
- deep tracing: when enabled logs from major events:

  * disks read/write;
  * system read/write;
  * kmalloc/kmem;
  * mm-page;

The application entry point is a `toml` file that contains a list of programs and general
settings. For example, it looks like:

.. code:: toml

  [globals]
  sample_size = 1
  output_directory = "demo-result"
  extra_perf_events = ["cpu-clock"]
  debug = false
  deep_trace = true

  [[tasks]]
  executable = "/usr/bin/sysbench"
  args = ["--threads={{ num_threads }}", "cpu", "run"]
  enclave_size = ["1G"]
  num_threads = [1, 2, 4]
  env = { OMP_NUM_THREADS = "{{ num_threads }}" }

  [[tasks]]
  executable = "/bin/dd"
  args = ["if=/dev/random", "of={{ output_directory }}/a.random", "count=1000000"]
  storage_type = ["encrypted", "untrusted"]
  enclave_size = ["256M", "512M"]

  post_run_executable = "rm"
  post_run_args = ["-rf", "{{ output_directory }}/a.random"]

The program will generate `len(<enclave_size>) x len(num_threads)` (2 x 3 = 6) 
experiments. Each experiment will be executed `sample_size` times. For Gramine,
the `storage_type` factor is included. The Gramine application will execute 
`len(<enclave_size>) x len(num_threads) x len(storage_type)` (2 x 3 x 3 = 18) times.

The `toml` file is dynamic. For example, if an application executes with a different 
number of threads, you can mark the parameter with the `{{ num_threads }}` placeholder.
On each iteration, it will be populated with an element from `globals.num_threads`
(see the `make` task in the example above). `{{ output_directory }}` is replaced in each
experiment with different paths from `storage_type` (non-Gramine applications always
have a simple storage).

Every run takes also an option `env` field which will be used to populate the process environment.

If `deep_trace = true`, the application is executed one more time collecting extra tracing metrics.
.. index:: eBPF

Low-level tracing
-----------------

Extra performance counters (like SGX or disk-related metrics) are collected 
leveraging the tracing system in the Linux kernel through **eBPF**. eBPF 
needs to be enabled in the kernel with configuration (should be already enabled in common
kernels) https://github.com/iovisor/bcc/blob/master/docs/kernel_config.md.

.. tip::
  eBPF is a technology in the Linux kernel that allows for the execution of 
  sandboxed programs within the kernel space. BPF programs run in a JIT 
  runtime with no heap and a very small stack. These programs are guaranteed 
  not to crash the kernel. More information can be found at https://docs.ebpf.io/.

Basically, eBPF programs are functions (like the one below) attached to specific
events by the kernel. Debug events are already available in the classical tracing system 
under `/sys/kernel/debug` (mounted as a `debugfs`) and need specific kernel 
compilation flags to be available.

.. code:: c

  static __always_inline int record_start_ts() {
    u32 pid;
    u64 ts;

    pid = (u32)bpf_get_current_pid_tgid();

    if (targ_pid && targ_pid != pid) {
      return 0;
    }
    ts = bpf_ktime_get_ns();

    bpf_map_update_elem(&start_ts_map, &pid, &ts, BPF_ANY);

    return 0;
  }
  SEC("tracepoint/syscalls/sys_enter_read")
  int trace_enter_read(struct trace_event_raw_sys_enter *ctx) {
    return record_start_ts();
  }

The function above is executed whenever a read system call is entered. Also, eBPF 
programs can store data in specific data structures which are included in the `maps`
section of the final binary. For example, the function above is updating an entry in a 
map which is declared like:

.. code:: c

  struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 1024);
    __type(key, u32);  
    __type(value, u64); 
  } start_ts_map SEC(".maps");

The application uses eBPF to collect I/O metrics like disk access patterns (sequential vs 
random) and the average duration of **read** and **write** operations and stores them in a file called 
`io.csv`.

For SGX functions, **kprobe** (https://docs.kernel.org/trace/kprobes.html) can be used to 
trace functions (the list can be obtained by running 
`cat /sys/kernel/debug/tracing/available_filter_functions | grep sgx`) and can be
inspected with the following program.

.. code:: c

  struct {
    __uint(type, BPF_MAP_TYPE_ARRAY);
    __uint(max_entries, 1);
    __type(key, u32);
    __type(value, u64);.
  } sgx_vma_access_counter SEC(".maps");

  SEC("kprobe/sgx_vma_access")
  int count_sgx_vma_access(struct pt_regs *ctx) {
    u32 key = 0;
    u64 *counter = bpf_map_lookup_elem(&sgx_vma_access_counter, &key);
    if (counter) {
        __sync_fetch_and_add(counter, 1);
    }

    return 0;
  }

Extra metrics
^^^^^^^^^^^^^
When `deep_trace = true`, the application logs system events regaring memory. This 
is achieved leveraging the `BPF_MAP_RINGBUF` data structure implemented in the Linux 
kernel. The ringbuffer sends objects from kernel to user space aynchronously. The 
example is taken from `src/bpf/tracer.bpf.c`.

.. code:: c

  struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1 << 20);
  } events SEC(".maps");

  static __always_inline int snd_trace_event(__u32 evt) {
    u32 pid = (u32)bpf_get_current_pid_tgid();

    u64 ts = bpf_ktime_get_ns();
    struct event *rb_event =
        bpf_ringbuf_reserve(&events, sizeof(struct event), 0);

    if (!rb_event) {
      bpf_printk("bpf_ringbuf_reserve failed\n");
      return 1;
    }

    rb_event->ev_type = evt;
    rb_event->timestamp = ts;

    bpf_ringbuf_submit(rb_event, 0);

    return 0;
  }

Gramine specific metrics
^^^^^^^^^^^^^^^^^^^^^^^^
Using `sgx.profile.mode = "ocall_outer"` and `sgx.enabled_stats = true` in a Gramine 
manifest enables extra performance counters which are printed to stderr. The application
collects these metrics and includes them in the `io.csv`. These metrics are reported below and 
are explained in https://gramine.readthedocs.io/en/stable/performance.html.

.. code:: sh

  ----- Total SGX stats for process 87219 -----
  # of EENTERs:        224
  # of EEXITs:         192
  # of AEXs:           201
  # of sync signals:   32
  # of async signals:  0


Disk Access Pattern Calculation
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

The following C code demonstrates how disk access patterns are calculated by comparing
the starting sector of the current I/O request with the ending sector of the previous
request. This is used in the eBPF code.

.. note::

  Disk access pattern is taken from here https://github.com/eunomia-bpf/bpf-developer-tutorial/tree/main/src/17-biopattern


.. code-block:: c

    SEC("tracepoint/block/block_rq_complete")
    int handle__block_rq_complete(struct trace_event_raw_block_rq_completion *ctx) {
      struct disk_counter *counterp, zero = {};
      sector_t sector;
      u32 nr_sector;
      u32 dev;
      __u32 ev_type = (ctx->rwbs[0] == 'R') ? EVENT_READ_DISK : EVENT_WRITE_DISK;
  
      if (deep_trace && snd_trace_event(ev_type)) {
          return 1;
      }
  
      sector = BPF_CORE_READ(ctx, sector);
      nr_sector = BPF_CORE_READ(ctx, nr_sector);
      dev = BPF_CORE_READ(ctx, dev);
  
      counterp = bpf_map_lookup_or_try_init(&counters, &dev, &zero);
      if (!counterp)
          return 0;
      if (counterp->last_sector) {
          if (counterp->last_sector == sector)
              __sync_fetch_and_add(&counterp->sequential, 1);
          else
              __sync_fetch_and_add(&counterp->random, 1);
          __sync_fetch_and_add(&counterp->bytes, nr_sector * 512);
      }
      counterp->last_sector = sector + nr_sector;
      return 0;
    }

1. **Extracting Request Data**:
   - The starting sector (`sector`), the number of sectors (`nr_sector`), and the device identifier (`dev`) are read from the context using the `BPF_CORE_READ` macro.
   
2. **Maintaining Disk Counters**:
   - The code uses a BPF map to retrieve or initialize a `disk_counter` structure for each device.
   - This structure tracks the last processed sector (`last_sector`), as well as counters for sequential and random disk accesses, and the total number of bytes processed.
   
3. **Determining the Access Pattern**:
   - If there is a previously recorded sector (`last_sector` is non-zero), the code compares it with the current `sector`.
   - If they are equal, it increments the sequential access counter.
   - Otherwise, it increments the random access counter.
   - The total bytes processed are updated by multiplying the number of sectors by 512 (bytes per sector).

4. **Updating the Last Processed Sector**:
   - The `last_sector` field is updated to `sector + nr_sector` after each I/O request, which serves as t


.. index:: Perf

Performance counters
--------------------

Default Linux performance counters are collected by attaching a ``perf`` process 
to the application pid and saving the results in a ``csv`` file called ``perf.csv``.
As trace events, performance counters need to be enabled in the kernel with specific 
configuration:

- CONFIG_PERF_EVENT
- CONFIG_HW_PERF_EVENTS
- CONFIG_PROFILING

.. tip::
 perf is a CLI utility provided by the Linux kernel to collect performance
 counters and profile applications. A full list of available counters
 (which may change depending on the platform) can be obtained by running 
 ``perf list``. More info on https://perfwiki.github.io/main/

The application spawns a perf process which is equivalent to running the following
command in the terminal:

.. code:: sh

   perf stat --field-separator , -e <some-events> --pid <PID>

Using the ``globals.extra_perf_events`` argument, it is possible to extend the default 
list of parameters in ``src/constants.rs`` For example:

.. code:: toml

   [globals]
   extra_perf_events = ["cpu-cycles"]

.. index:: RAPL

Energy measurement
------------------
Energy measurement is performed through sampling using `globals.energy_sample_interval`.
Energy data is collected leveraging the **Running Average Power Limit (RAPL)** technology
implemented in the Linux kernel.

.. tip::
 The RAPL interface proposed by Intel is supported also by AMD. Linux divides the platform
 into **power domains** accessible with a sysfs tree. More info on 
 https://www.kernel.org/doc/html/next/power/powercap/powercap.html

An Intel-RAPL hierarchy may look like this:

.. code:: sh

  /sys/devices/virtual/powercap/
  └── intel-rapl
      ├── enabled
      ├── intel-rapl:0
      │   ├── device -> ../../intel-rapl
      │   ├── enabled
      │   ├── energy_uj
      │   ├── intel-rapl:0:0
      │   │   ├── device -> ../../intel-rapl:0
      │   │   ├── enabled
      │   │   ├── energy_uj
      │   │   ├── max_energy_range_uj
      │   │   ├── name
      │   │   ├── power
      │   │   │   ├── autosuspend_delay_ms
      │   │   │   ├── control
      │   │   │   ├── runtime_active_time
      │   │   │   ├── runtime_status
      │   │   │   └── runtime_suspended_time
      │   │   ├── subsystem -> ../../../../../../class/powercap
      │   │   └── uevent
      │   ├── max_energy_range_uj
      │   ├── name
      │   ├── power
      │   │   ├── autosuspend_delay_ms
      │   ├── control
      │   │   ├── runtime_active_time
      │   │   ├── runtime_status
      │   │   └── runtime_suspended_time
      │   ├── subsystem -> ../../../../../class/powercap
      │   └── uevent
      ├── power
      │   ├── autosuspend_delay_ms
      │   ├── control
      │   ├── runtime_active_time
      │   ├── runtime_status
      │   └── runtime_suspended_time
      ├── subsystem -> ../../../../class/powercap
      └── uevent

A RAPL domain is in the form of *intel-rapl:i:j* where *i* is a CPU package (power zones)
and *j* a subzone. In each node, a file `name` indicates the component name:

- intel-rapl:0 -> package-0
- intel-rapl:0:0 -> core (all components internal to the CPU that perform computations)
- intel-rapl:0:1 -> uncore (all components internal to the CPU that do not perform 
  computations, like caches)
- intel-rapl:0:2 -> dram

The application reads the `energy_uj` file which contains an energy counter corresponding 
to microjoules. 

The application creates a `csv` file in the form of `<package>-<component>.csv` with 2 
columns:

- timestamp: when the measurement occurred in nanoseconds;
- microjoule: value of the `energy_uj` file 

Disk energy consumption
^^^^^^^^^^^^^^^^^^^^^^^
It's very hard to determine disk energy consumption as there is no Linux standard. 
An estimation can be made using the aggregated counters and the `deep_trace` execution.
Based on disk model, specification can say what is the average power consumption of  
writing/reading a block. This information can be combined with read/write counters to 
obtain useful metrics.

Interfacing with Gramine
------------------------
Gramine is a toolkit to convert already existing applications into enclaves using SGX. Every 
Gramine application is based on a `manifest` which contains the description of the
application and facilitates trusted files, disk encryption, and OS separation. The 
manifest is a TOML file that can be preprocessed using Jinja2 templates.

Building a Gramine application from Rust
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
Gramine provides a Python library to automate the process of creating Enclaves. 
Using PyO3, the application uses the `graminelibos` Python library and builds enclaves 
from a default manifest included in `src/constants.rs`. Building a Gramine-SGX 
application means:

- parsing the manifest.template to a manifest file (expanding all variables)
- expanding all trusted files (calculating hashes)
- signing the manifest and performing the measurement of the application

According to `Gramine <https://github.com/iovisor/bcc/blob/master/docs/kernel_config.md>`_
an enclave can be built and signed with:

.. code:: python
  
  import datetime
  from graminelibos import Manifest, get_tbssigstruct, sign_with_local_key

  with open('some_manifest_template_file', 'r') as f:
    template_string = f.read()

  # preprocess using Jinja2
  manifest = Manifest.from_template(template_string, {'foo': 123})

  with open('some_output_file', 'w') as f:
    manifest.dump(f)

  today = datetime.date.today()
  # Manifest must be ready for signing, e.g. all trusted files must be already expanded.
  sigstruct = get_tbssigstruct('path_to_manifest', today, 'optional_path_to_libpal')
  sigstruct.sign(sign_with_local_key, 'path_to_private_key')

  with open('path_to_sigstruct', 'wb') as f:
    f.write(sigstruct.to_bytes())

For each experiment, the application builds the following structure:

.. code:: sh

  ├── <prog>-<threads>-<enclave-size>-<storage>
  │   └── 1
  ├── <prog>.manifest.sgx
  ├── <prog>.sig
  ├── encrypted
  └── untrusted

The root directory is the `experiment_directory` which contains:

- **<prog>.manifest.sgx**: the built manifest which contains all trusted files' hashes, mount points
  etc.;
- **<prog>.sig**: contains the enclave signature;
- **encrypted**: a directory mounted as encrypted to the Gramine application. Every file
  will be protected by a hardcoded key;
- **untrusted**: a directory mounted to the enclave as `sgx.allowed_files`

Untrusted and encrypted path directories will be used by the user through the 
`{{ output_directory }}` variable in the input file.

Every iteration specified in `globals.sample_size` will have a dedicated directory 
(called with the index of the iteration) in `<prog>-<threads>-<enclave-size>-<storage>`.
