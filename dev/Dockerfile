FROM ubuntu:24.04

WORKDIR /tmp

# install dependencies
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get -y install build-essential clang \
  clang llvm-dev python3-dev libbpf-dev git clang autoconf bison gawk meson nasm \
  pkg-config python3 python3-click python3-jinja2 python3-pyelftools  python3-tomli \
  python3-tomli-w python3-voluptuous wget cmake libprotobuf-c-dev protobuf-c-compiler \
  protobuf-compiler python3-cryptography python3-pip python3-protobuf curl flex \
  libzstd1 libdwarf-dev libdw-dev binutils-dev libcap-dev libelf-dev libnuma-dev libssl-dev \
  libunwind-dev libdwarf-dev zlib1g-dev liblzma-dev libaio-dev libtraceevent-dev debuginfod \
  libpfm4-dev libslang2-dev systemtap-sdt-dev libperl-dev binutils-dev libbabeltrace-dev \
  libiberty-dev libzstd-dev sysbench 

# build perf against current kernel version 
# depending on the underling distro 
# uname -r returns slightly different versions
RUN VERSION=$(uname -r | awk -F '_' '{print $1}' | awk -F '-' '{print $1}' | awk -F '.' '{if ($3 == 0) print $1 "." $2; else print $1 "." $2 "." $3}') && \
  MAJOR=$(uname -r | awk -F . '{print $1}') && \
  curl -O https://cdn.kernel.org/pub/linux/kernel/v$MAJOR.x/linux-$VERSION.tar.xz && \
  tar -xvf linux-$VERSION.tar.xz && \
  make -C linux-$VERSION/tools/perf install && \
  make -C linux-$VERSION/tools/bpf bpftool_install && \
  rm -rf linux-$VERSION linux-$VERSION.tar.xz

# configure intel sgx sdk
RUN curl -fsSLo /usr/share/keyrings/intel-sgx-deb.asc https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key && \
  echo "deb [arch=amd64 signed-by=/usr/share/keyrings/intel-sgx-deb.asc] https://download.01.org/intel-sgx/sgx_repo/ubuntu noble main" \
  | tee /etc/apt/sources.list.d/intel-sgx.list

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y libsgx-epid libsgx-quote-ex libsgx-dcap-ql libsgx-dcap-quote-verify-dev

# download and build gramine
WORKDIR /workspace
RUN curl -O -L https://github.com/gramineproject/gramine/archive/refs/tags/v1.8.tar.gz
RUN tar xvf v1.8.tar.gz
RUN mv gramine-1.8 /workspace/gramine 
RUN rm v1.8.tar.gz

WORKDIR /workspace/gramine
RUN meson setup build/ --buildtype=debugoptimized -Ddirect=enabled -Dsgx=enabled -Ddcap=enabled

RUN ninja -C build/
RUN ninja -C build/ install

# avoid "Signing key does not exist" error
RUN gramine-sgx-gen-private-key

# add user to sgx_prv to access remote attestation primitives
RUN groupadd sgx_prv
RUN usermod -aG sgx_prv root

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

RUN echo 'source $HOME/.cargo/env' >> /root/.bashrc
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /workspace
