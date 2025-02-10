#/bin/sh

# Purpose of this script is to setup a quick development environment in an Ubuntu 24.04 
# machine. 
# - install required dependencies, 
# - install perf 
# - install bpftool 
# - builds and install Gramine from source 
# - install Rust toolchain

if [ "$(id -u)" -ne 0 ]; then
  echo "This script must be run as root" >&2
  exit 1
fi

if [ ! $SUDO_USER ]; then 
  echo "This script must be run with sudo not with root"
  exit 1
fi

USER_NAME="$SUDO_USER" # Replace with your actual username
USER_HOME=$(eval echo ~$USER_NAME) # Get user's home directory

echo "Running the script as $USER_NAME"
# setup dependencies
apt-get update && DEBIAN_FRONTEND=noninteractive apt-get -y install build-essential clang \
  clang llvm-dev python3-dev libbpf-dev git clang autoconf bison gawk meson nasm \
  pkg-config python3 python3-click python3-jinja2 python3-pyelftools python3-tomli \
  python3-tomli-w python3-voluptuous wget cmake libprotobuf-c-dev protobuf-c-compiler \
  protobuf-compiler python3-cryptography python3-pip python3-protobuf curl linux-tools-`uname -r`

# install intel sgx core libraries
echo 'deb [signed-by=/etc/apt/keyrings/intel-sgx-keyring.asc arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu noble main' | \
tee /etc/apt/sources.list.d/intel-sgx.list  

sudo -u $USER_NAME wget https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key
cat intel-sgx-deb.key| tee /etc/apt/keyrings/intel-sgx-keyring.asc > /dev/null

apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
  libsgx-dcap-quote-verify-dev libsgx-epid libsgx-quote-ex libsgx-dcap-ql

sudo -u $USER_NAME rm intel-sgx-deb.key

# get gramine
sudo -u $USER_NAME git clone --depth=1 --branch v1.8 https://github.com/gramineproject/gramine.git /tmp/gramine-src
cd /tmp/gramine-src
sudo -u $USER_NAME git checkout v1.8 

# build and install gramine
sudo -u $USER_NAME meson setup build/ --buildtype=debugoptimized -Dsgx=enabled -Ddcap=enabled -Dlibc=glibc
sudo -u $USER_NAME ninja -C build/
ninja -C build/ install

# install rust toolchain
su - $USER_NAME -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y"
