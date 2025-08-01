#!/bin/sh

curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | sh

cargo binstall --locked taplo-cli cargo-make

chown -R ${USERNAME}:${USERNAME} /usr/local/cargo
