FROM ubuntu:24.04

WORKDIR /tmp

# install dependencies
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
  vim \
  curl \
  flex \
  gnupg2 \
  cmake \
  psmisc \
  make \
  build-essential \
  binutils-dev \
  autoconf \ 
  bison \
  gawk \
  nasm \
  ninja-build \
  pkg-config \
  meson \
  protobuf-c-compiler \
  python3 \ 
  python3-click \
  python3-jinja2 \
  python3-pip \
  python3-pyelftools \
  python3-setuptools \
  python3-tomli \
  python3-tomli-w \
  python3-voluptuous \
  python3-cryptography \
  python3-protobuf \
  python3-dev \
  libprotobuf-c-dev \
  libzstd1 \ 
  libdwarf-dev \
  libdw-dev \
  libcap-dev \
  libelf-dev \
  libnuma-dev \
  libssl-dev \
  libdwarf-dev \
  zlib1g-dev \
  liblzma-dev \
  libaio-dev \
  libtraceevent-dev \
  libtracefs-dev \
  debuginfod \
  libpfm4-dev \
  libslang2-dev \
  systemtap-sdt-dev \
  libperl-dev \
  libbabeltrace-dev \
  libiberty-dev \
  libunwind-dev \
  libzstd-dev

# build perf against current kernel version 
# depending on the underling distro 
# uname -r returns slightly different versions
RUN VERSION=$(uname -r | awk -F '_' '{print $1}' | awk -F '-' '{print $1}' | awk -F '.' '{if ($3 == 0) print $1 "." $2; else print $1 "." $2 "." $3}') && \
  MAJOR=$(uname -r | awk -F . '{print $1}') && \
  echo $(uname -r) VERSION=$VERSION MAJOR=$MAJOR && \
  curl -O https://cdn.kernel.org/pub/linux/kernel/v$MAJOR.x/linux-$VERSION.tar.xz && \
  tar -xvf linux-$VERSION.tar.xz && \
  make -C linux-$VERSION/tools/perf install DESTDIR=/usr/local

# configure intel sgx sdk
RUN curl -fsSLo /usr/share/keyrings/intel-sgx-deb.asc https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key && \
  echo "deb [arch=amd64 signed-by=/usr/share/keyrings/intel-sgx-deb.asc] https://download.01.org/intel-sgx/sgx_repo/ubuntu noble main" \
  | tee /etc/apt/sources.list.d/intel-sgx.list

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y libsgx-epid libsgx-quote-ex libsgx-dcap-ql

RUN curl -O https://download.01.org/intel-sgx/sgx-dcap/1.22/linux/distro/ubuntu24.04-server/sgx_linux_x64_sdk_2.25.100.3.bin
RUN chmod +x sgx_linux_x64_sdk_2.25.100.3.bin
RUN ./sgx_linux_x64_sdk_2.25.100.3.bin --prefix /opt/intel/
RUN rm sgx_linux_x64_sdk_2.25.100.3.bin

RUN DEBIAN_FRONTEND=noninteractive apt-get install -y \
  libsgx-enclave-common-dev \
  libsgx-dcap-ql-dev \
  libsgx-dcap-default-qpl-dev \
  libsgx-dcap-quote-verify-dev

# download and build gramine
WORKDIR /workspace
RUN curl -O -L https://github.com/gramineproject/gramine/archive/refs/tags/v1.8.tar.gz
RUN tar xvf v1.8.tar.gz
RUN mv gramine-1.8 /workspace/gramine 
RUN rm v1.8.tar.gz

WORKDIR /workspace/gramine
RUN meson setup build/ \
  --buildtype=debugoptimized \
  -Ddirect=enabled \ 
  -Dsgx=enabled \
  -Ddcap=enabled

RUN ninja -C build/
RUN ninja -C build/ install

RUN echo "source /opt/intel/sgxsdk/environment" >> /root/.bashrc

# avoid "Signing key does not exist" error
RUN gramine-sgx-gen-private-key

# configure AESM - Architectural Enclaves Service Manager
RUN echo "#!/bin/sh \n \
  set -e \n \
  killall -q aesm_service || true \n \
  AESM_PATH=/opt/intel/sgx-aesm-service/aesm LD_LIBRARY_PATH=/opt/intel/sgx-aesm-service/aesm exec /opt/intel/sgx-aesm-service/aesm/aesm_service --no-syslog \n\
  " >> /restart_aesm.sh

RUN mkdir -p /var/run/aesmd
RUN chmod +x /restart_aesm.sh
RUN /restart_aesm.sh

# add user to sgx_prv to access remote attestation primitives
RUN groupadd sgx_prv
RUN usermod -aG sgx_prv root

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

RUN echo 'source $HOME/.cargo/env' >> /root/.bashrc

