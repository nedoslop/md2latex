@echo off
set RUSTFLAGS=-C target-feature=+crt-static --remap-path-prefix =/ -C link-arg=/DEBUG:NONE
set CRT_STATIC=true
cargo build --release
set CRT_STATIC=
set RUSTFLAGS=
