# Building Depot

## Requirements

- Rust 1.70+
- CMake (for aws-lc-sys)

## Debug Build

```bash
cargo build
```

## Release Build

```bash
cargo build --release
```

The binary will be at `target/release/depot`.

## Cross-Compilation

Build for different targets:

```bash
# Windows (GNU toolchain)
cargo build --release --target x86_64-pc-windows-gnu

# Windows (MSVC toolchain)
cargo build --release --target x86_64-pc-windows-msvc

# Linux
cargo build --release --target x86_64-unknown-linux-gnu

# macOS Intel
cargo build --release --target x86_64-apple-darwin

# macOS Apple Silicon
cargo build --release --target aarch64-apple-darwin
```

## Build Dependencies

### Linux (Debian/Ubuntu)

```bash
sudo apt-get install build-essential cmake
```

### macOS

```bash
xcode-select --install
brew install cmake
```

### Windows

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) with C++ workload, or use:

```powershell
choco install cmake visualstudio2022buildtools
```

## Optimizing for Size

The release build uses the following optimizations in `Cargo.toml`:

```toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Single codegen unit for better optimization
panic = "abort"     # Remove panic unwinding code
strip = true        # Strip symbols
```

## Running Tests

```bash
# Unit tests
cargo test

# Integration tests (requires smbclient)
./tests/smb_test.sh
./tests/ftp_test.sh
./tests/http_test.sh
./tests/cli_test.sh
./tests/auth_test.sh
```
