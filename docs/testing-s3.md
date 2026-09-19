# Running the S3 integration tests

The S3 backend is tested against a real object store rather than a mock:
[RustFS](https://rustfs.com), the Rust successor to MinIO. The binary is not
vendored, so the tests skip themselves unless it is available.

## One-time setup

```sh
mkdir -p ~/.cache/d2mz-test && cd ~/.cache/d2mz-test
curl -LO https://github.com/rustfs/rustfs/releases/download/1.0.0/rustfs-linux-x86_64-gnu-latest.zip
unzip rustfs-linux-x86_64-gnu-latest.zip
curl -LO https://github.com/rustfs/cli/releases/download/v0.1.36/rustfs-cli-linux-amd64-gnu-v0.1.36.tar.gz
tar xzf rustfs-cli-linux-amd64-gnu-v0.1.36.tar.gz
```

## Run

```sh
D2MZ_RUSTFS_BIN=~/.cache/d2mz-test/rustfs \
D2MZ_RUSTFS_CLI=~/.cache/d2mz-test/rc \
cargo test --features test-support -- --test-threads=1
```

The harness starts RustFS on an ephemeral port, creates the `d2mz-it`
bucket, and tears the server down when the test ends. `D2MZ_RUSTFS_CLI` is
optional; without it the tests seed objects through d2mz's own operator.

Without `D2MZ_RUSTFS_BIN` the S3 tests print a skip notice and pass.
