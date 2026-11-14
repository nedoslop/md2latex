@echo off
set RUSTFLAGS=-C target-feature=+crt-static --remap-path-prefix =/ -C link-arg=/DEBUG:NONE
cargo build --release
set RUSTFLAGS=
