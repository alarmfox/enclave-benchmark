#!/bin/bash
#
# This is an install script for Ubuntu-specific packages.
#
set -ex

export DEBIAN_FRONTEND=noninteractive

apt-get update
apt-get -y dist-upgrade

PACKAGES=(build-essential pkg-config clang meson libgl-dev libglew-dev libsdl2-dev libglm-dev)

apt-get install -y "${PACKAGES[@]}"

# Optional: try to install libomp-dev. It doesn't exist for the CUDA 7.5 docker
# environment (which is based on Trusty).
apt-get install -y libomp-dev || true
