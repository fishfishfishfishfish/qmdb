#!/bin/bash
sudo apt-get install g++ linux-libc-dev libclang-dev unzip libjemalloc-dev make -y
curl https://sh.rustup.rs -sSf | sudo sh # install cargo
source ~/.cargo/env
cargo install cargo-nextest