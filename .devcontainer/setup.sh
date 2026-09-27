#!/bin/bash
set -e

# System deps cho Tauri + WebKitGTK
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl wget file libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  xvfb \
  webkit2gtk-driver \
  dbus-x11

# Rust target WASM
rustup target add wasm32-unknown-unknown

# Trunk + Tauri CLI
cargo install trunk --locked
cargo install tauri-cli --version "^2.0.0" --locked
