# ESP32-C6 SGP41 VOC/NOx Project

# Default recipe - run the project
default: run

# Build the project
build:
    cargo build

# Run the project on ESP32-C6 with RTT logging
run:
    cargo run

# Build in release mode
build-release:
    cargo build --release

# Run in release mode
run-release:
    cargo run --release

# Clean build artifacts
clean:
    cargo clean

# Format code
fmt:
    cargo fmt

# Run clippy linter
clippy:
    cargo clippy

# Check code without building
check:
    cargo check

# Flash and monitor using espflash (alternative to cargo run)
flash:
    espflash flash --monitor target/riscv32imac-unknown-none-elf/debug/esp-sgp41-VOC-NOx

# Flash release build with espflash
flash-release:
    espflash flash --monitor target/riscv32imac-unknown-none-elf/release/esp-sgp41-VOC-NOx

# List connected probes
list-probes:
    probe-rs list

# Show help
help:
    @just --list
