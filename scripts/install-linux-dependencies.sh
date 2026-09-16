#!/usr/bin/env bash
set -euo pipefail

if ! command -v apt-get >/dev/null 2>&1; then
  echo "This installer supports Debian and Ubuntu (apt-get) only." >&2
  exit 1
fi

sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libxdo-dev \
  patchelf \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libasound2-dev \
  libvulkan1 \
  mesa-vulkan-drivers \
  vulkan-tools \
  libvulkan-dev

echo "Linux system dependencies installed. Node.js 18+, npm, Rust, and CMake 3.20+ are also required."
