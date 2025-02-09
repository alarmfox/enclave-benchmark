Installation
============

Requirements 
------------

This is setup page

Host requirements
^^^^^^^^^^^^^^^^^


OS Dependencies
^^^^^^^^^^^^^^^

Host setup
^^^^^^^^^^

Building Gramine
""""""""""""""""

We need to build Gramine from source to get access to debug information at runtime. 

.. code:: sh

  meson setup build/ \
    --buildtype=debugoptimized \
    -Ddirect=enabled \ 
    -Dsgx=enabled \
    -Ddcap=enabled

Building from source
--------------------
Currently, the application can be installed **only** from source as it heavily 
depends on the host operating system.

First, get a copy of the source code using:

.. code:: sh 

   git clone https://github.com/alarmfox/enclave-benchmark.git

Install the rust toolchain from `here <https://rustup.rs/>`_. Which will look like 
(`curl` required) this (follow the instructions).

.. code:: sh

  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

Next, generate a `vmlinux.h` (required to compile eBPF programs)

.. code:: sh

  bpftool btf dump file /sys/kernel/btf/vmlinux format c > src/bpf/vmlinux.h


Now, you can run the build command (remove the `--release` for a fast but unoptimized
build):

.. code:: sh

  cargo build --release

**(Optional)** Copy the executable somewhere else:

.. code:: sh
   
  cp target/<debug|release>/enclave-benchmark .

Run the application:

.. code:: sh

  ./enclave-benchmark -h 

  A cli app to run benchmarks for Gramine application

  Usage: enclave-benchmark [OPTIONS] --config <CONFIG>

  Options:
    -v...                  Turn debugging information on
    -c, --config <CONFIG>  Path to configuration file
        --force            Remove previous results directory (if exists)
    -h, --help             Print help
    -V, --version          Print version
