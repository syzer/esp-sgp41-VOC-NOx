# ESP32-C6 SGP41 VOC/NOx Sensor Project

A Rust project for ESP32-C6 microcontroller that interfaces with SGP41 VOC (Volatile Organic Compounds) and NOx (Nitrogen Oxides) sensors using Embassy async framework and Bluetooth Low Energy connectivity.

## Result

```bash
SGP41 Raw Measurements:
INFO    VOC Raw: 30079 ticks
INFO    NOx Raw: 17753 ticks
INFO    VOC Index (approx): 5.0
INFO    NOx Index (approx): 0.0
```
Office is not that great, but the sensor is working and the code is ready for further development.

https://sensirion.com/media/documents/02232963/6294E043/Info_Note_VOC_Index.pdf
```bash
while a VOC Index below 100 means
that there are fewer VOCs compared to the average (e.g., induced by
fresh air from an open window, using an air purifier, etc.).
```

## Features

- **Embassy Async Framework**: Modern async/await support for embedded systems
- **Bluetooth Low Energy**: Built-in BLE controller support
- **Real-Time Logging**: RTT (Real-Time Transfer) logging with `defmt`
- **SGP41 Sensor Integration**: VOC and NOx environmental monitoring
- **ESP32-C6 Optimized**: Configured for ESP32-C6 RISC-V architecture

## Prerequisites

Before running this project, ensure you have:

1. **Rust** with ESP32 targets installed
2. **probe-rs** or **espflash** for flashing and debugging
3. **just** command runner (optional but recommended)

```bash
# Install just
cargo install just

# Install probe-rs
cargo install probe-rs --features=cli

# Install espflash (alternative flasher)
cargo install espflash
```

## Quick Start

### Using just (recommended)

```bash
# Run the project (builds and flashes to ESP32-C6)
just run

# Or simply
just

# Build only
just build

# Clean build artifacts
just clean

# Format code
just fmt

# Run linter
just clippy
```

### Using cargo directly

```bash
# Run the project
cargo run

# Build only
cargo build
```

### Alternative: Using espflash

```bash
# Flash and monitor with espflash
just flash

# Or manually
espflash flash --monitor target/riscv32imac-unknown-none-elf/debug/esp-sgp41-VOC-NOx
```

## Project Structure

```
├── src/
│   ├── lib.rs              # Library code
│   └── bin/
│       └── main.rs         # Main application entry point
├── tests/
│   └── hello_test.rs       # Test files
├── Cargo.toml              # Project dependencies and configuration
├── justfile                # Just command recipes
└── README.md               # This file
```

## Viewing Logs

The project uses RTT (Real-Time Transfer) for logging. When you run `cargo run` or `just run`, you'll see output like:

```
Embassy initialized!
Hello world!
Hello world!
Hello world!
...
```

The "Hello world!" message prints every second as configured in the main loop.

## Available Just Commands

| Command | Description |
|---------|-------------|
| `just` or `just run` | Build and run the project |
| `just build` | Build the project |
| `just build-release` | Build in release mode |
| `just run-release` | Run release build |
| `just clean` | Clean build artifacts |
| `just fmt` | Format code |
| `just clippy` | Run clippy linter |
| `just check` | Check code without building |
| `just flash` | Flash using espflash |
| `just flash-release` | Flash release build |
| `just list-probes` | List connected debug probes |
| `just help` | Show all available commands |

## Hardware Setup

1. Connect your ESP32-C6 development board via USB
2. Ensure the SGP41 sensor is properly wired (I²C connection)
3. Power on the device

## Troubleshooting

### Probe Connection Issues

If `cargo run` fails with probe connection errors:

1. Check if device is connected: `just list-probes`
2. Try alternative flasher: `just flash`
3. Ensure no other programs are using the USB port
4. Check USB cable and connections

### Build Issues

- Ensure you have the correct Rust targets: `rustup target add riscv32imac-unknown-none-elf`
- Update dependencies: `cargo update`
- Clean and rebuild: `just clean && just build`

## Development

The main application logic is in `src/bin/main.rs`. The project is set up with:

- Embassy executor for async task management
- WiFi/BLE initialization
- RTT logging with `defmt`
- 1-second timer loop for periodic operations

To add SGP41 sensor functionality, modify the main loop to include sensor reading and data processing.

// for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-beta.1/examples/src/bin

```bash
screen /dev/cu.usbserial-110 115200
SRAW_VOC 30309  SRAW_NOx 15949
VOC Index 106   NOx Index 1
SRAW_VOC 30307  SRAW_NOx 15952
VOC Index 106   NOx Index 1
SRAW_VOC 30307  SRAW_NOx 15948
VOC Index 106   NOx Index 1
SRAW_VOC 30299  SRAW_NOx 15952
VOC Index 106   NOx Index 1
SRAW_VOC 30298  SRAW_NOx 15950
VOC Index 106   NOx Index 1
SRAW_VOC 30304  SRAW_NOx 15950
VOC Index 106   NOx Index 1
SRAW_VOC 30298  SRAW_NOx 15946
VOC Index 106   NOx Index 1
SRAW_VOC 30301  SRAW_NOx 15947
VOC Index 106   NOx Index 1
SRAW_VOC 30297  SRAW_NOx 15943
VOC Index 106   NOx Index 1
SRAW_VOC 30310  SRAW_NOx 15945
VOC Index 106   NOx Index 1
SRAW_VOC 30307  SRAW_NOx 15947
VOC Index 106   NOx Index 1
SRAW_VOC 30302  SRAW_NOx 15942
VOC Index 105   NOx Index 1
SRAW_VOC 30297  SRAW_NOx 15941
VOC Index 105   NOx Index 1
SRAW_VOC 30297  SRAW_NOx 15937
VOC Index 105   NOx Index 1
SRAW_VOC 30298  SRAW_NOx 15937
VOC Index 105   NOx Index 1
SRAW_VOC 30307  SRAW_NOx 15933
VOC Index 105   NOx Index 1
SRAW_VOC 30303  SRAW_NOx 15937
VOC Index 105   NOx Index 1
SRAW_VOC 30308  SRAW_NOx 15931
VOC Index 105   NOx Index 1
SRAW_VOC 30302  SRAW_NOx 15930
VOC Index 105   NOx Index 1
SRAW_VOC 30313  SRAW_NOx 15935
VOC Index 104   NOx Index 1
SRAW_VOC 30302  SRAW_NOx 15927
VOC Index 104   NOx Index 1
SRAW_VOC 30307  SRAW_NOx 15927
VOC Index 104   NOx Index 1
SRAW_VOC 30305  SRAW_NOx 15929
VOC Index 104   NOx Index 1
SRAW_VOC 30302  SRAW_NOx 15927
VOC Index 104   NOx Index 1
SRAW_VOC 30309  SRAW_NOx 15927
VOC Index 104   NOx Index 1
```