How does it work?
=================

For each task, the application builds a series of experiments based on `globals.num_threads`,
`globals.enclave_size`, and `task.storage` (for Gramine runs only).

The application launches the target program in a separate process and starts the 
monitoring in 3 threads:

- eBPF collector: runs eBPF programs and collects low-level stats;
- energy monitor: polls energy consumption;
- performance counters: runs perf in a separate process;

The application entry point is a `toml` file that contains a list of programs and general
settings. For example, it looks like:

.. code:: toml

  [globals]
  sample_size = 4
  enclave_size = ["256M", "512M"]
  output_directory = "results"
  num_threads = [1, 2, 8]
  extra_perf_events = ["cpu-clock"]
  debug = false

  [[tasks]]
  executable = "/usr/bin/make"
  args = ["-C", "examples/basic-c-app/", "-j", "{{ num_threads }}", "app", "output={{ output_directory }}"]
  storage_type = ["encrypted", "tmpfs", "untrusted"]

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

  <prog>-<threads>-<enclave-size>/
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
