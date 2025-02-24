Installation
============

This section illustrates how to install the application. 

Since Rust is supported all major platforms and OS the real constraint is Gramine. 

Since, we are building Gramine from source (it is needed to profile applications) 
on every operating system supported by SGX Platform. Gramine 
relies on the SGX driver which is included in the kernel by default starting from
`5.11` version (with config `CONFIG_X86_SGX=y`).

.. note::

   The application has been tested with Ubuntu 24.04, Ubuntu 22.04 and Void Linux (with Musl)
   with Gramine v1.8.
   The following instructions setup apply **only** for Ubuntu 24.04. For further 
   support, refer to the `official guide <https://download.01.org/intel-sgx/latest/dcap-latest/linux/docs/Intel_SGX_SW_Installation_Guide_for_Linux.pdf>`_

Host setup
----------

There are 3 possibile setups:

- using the script in `dev/setup_host.sh` (working only on Ubuntu 22.04 and 24.04);
- installing manually dependencies on Ubuntu 24.04;
- creating a Docker image;

Using a script (Ubuntu 22.04 and 24.04 only)
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
A quick setup can be done using the script in `dev/setup_host.sh`:

.. code:: sh
   
  sudo ./dev/setup_host.sh

Bare metal
^^^^^^^^^^
The host needs Intel SGX software installed. First, we need to enabled SGX in the BIOS.
Then we need to install build dependencies. These are needed to build Gramine from source 
and the benchmark application.

First install build dependencies:

.. code:: sh
   
  sudo apt-get install -y  apt-get -y install build-essential clang \
  clang llvm-dev python3-dev libbpf-dev git clang autoconf bison gawk meson nasm \
  pkg-config python3 python3-click python3-jinja2 python3-pyelftools python3-tomli \
  python3-tomli-w python3-voluptuous wget cmake libprotobuf-c-dev protobuf-c-compiler \
  protobuf-compiler python3-cryptography python3-pip python3-protobuf curl linux-tools-`uname -r`

Then install SGX software:

.. code:: sh

  echo 'deb [signed-by=/etc/apt/keyrings/intel-sgx-keyring.asc arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu noble main' | \
  sudo tee /etc/apt/sources.list.d/intel-sgx.list  
  wget https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key
  cat intel-sgx-deb.key | sudo tee /etc/apt/keyrings/intel-sgx-keyring.asc > /dev/null
  sudo apt-get update
  sudo apt-get install libsgx-dcap-quote-verify-dev libsgx-epid libsgx-quote-ex libsgx-dcap-ql

Finally, we need a Rust toolchain. This is can be done from `rustup <https://rustup.rs/>`_:

.. code:: sh

  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

Building Gramine
""""""""""""""""

First, we need to get a copy of the source code (we are using v1.8). This can be done using `git`:

.. code:: sh
   
  git clone --depth=1 --branch v1.8 https://github.com/gramineproject/gramine.git
  cd gramine
  git checkout v1.8 

Configure, build and install Gramine using meson:

.. note::

   `buildtype` needs to be either `debug` or debugoptimized otherwise it will be not 
   possibile to profile Gramine applications. To use `musl <https://musl.libc.org/>`_
   pass `musl` to `-Dlibc` argument.

.. code:: sh

  meson setup build/ --buildtype=debugoptimized -Dsgx=enabled -Ddcap=enabled -Dlibc=glibc
  meson compile -C build/
  sudo meson compile -C build/ install

Creating a Docker container
^^^^^^^^^^^^^^^^^^^^^^^^^^^
If not on Ubuntu 24.04, it is possibile to create a Docker image using the Dockerfile in 
`dev/Dockerfile` by running:

.. code:: sh

   docker build -t enclave-benchmark-dev - <dev/Dockerfile 

The building process can be very long since it compiles `perf` and `libbf` from scratch using the 
host kernel source code. Finally, you can create a Docker container (`--privileged` is 
needed to access performance counters) passing **sgx devices** `/dev/sgx_enclave` :

.. code:: sh

   docker run -it -w /app -v $(pwd):/app --priviliged --device /dev/sgx_enclave --name enclave-benchmark enclave-benchmark-dev

Building from source
--------------------
Currently, the application can be installed **only** from source as it heavily 
depends on the host operating system.

First, get a copy of the source code using:

.. code:: sh 

   git clone https://github.com/alarmfox/enclave-benchmark.git

Install the rust toolchain from `here <https://rustup.rs/>`_. Which will look like 
(`curl` required) this (follow the instructions).

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

  ./enclave-benchmark -V

  enclave-benchmark 0.1.0

