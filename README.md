# d2mz

**Door 2 MeriaZard** — one CLI to browse and archive local and remote data
sources.

d2mz puts every backend (local files, S3-compatible object storage, and more
to come) behind a single virtual data source called **MZ**. The same commands
work no matter where the bytes live.

> Status: early. The read-only browse commands (`ls`, `cat`, `stat`, `find`)
> work against local files, S3-compatible storage, and plain HTTP(S) URLs. The
> content-addressed archive is designed but not implemented yet. The original
> Python prototype is preserved under [`legacy/`](legacy/).

## Build

```sh
cargo build --release
# binary at target/release/d2mz (~7.5M, static)
```

## Usage

A target is a URI. Bare paths mean the `local` backend.

```sh
d2mz ls                          # current directory
d2mz ls -l /var/log              # long listing
d2mz ls -R /var/log              # recursive
d2mz ls --json /var/log          # machine readable
d2mz cat ./notes.txt             # stream a file to stdout
d2mz stat ./notes.txt            # metadata
d2mz find ./src --name '*.rs'    # name glob below a URI
d2mz find . --min-size 1M        # size filter

d2mz ls mz://r2/backups/2026     # a named remote backend
d2mz list --json                 # (reserved)
```

`find` accepts a comma-separated glob (`--name '*.log,*.txt'`) and sizes
such as `10K`, `5M`, `2G`.

## Configuration

Read from `$D2MZ_CONFIG`, otherwise `<config-dir>/d2mz/config.toml`
(typically `~/.config/d2mz/config.toml`). A missing file is fine — a local
backend rooted at `/` is always available.

```toml
archive_dir = "~/.local/share/d2mz"

[[backend]]
name = "local"
scheme = "fs"
root = "/"

# RustFS / MinIO / R2 / AWS S3 all use the same `s3` scheme.
[[backend]]
name = "rfs"
scheme = "s3"
bucket = "mybucket"
endpoint = "http://127.0.0.1:9000"
region = "us-east-1"
access_key_id = "..."
secret_access_key = "..."

# A read-only web backend over HTTP(S).
[[backend]]
name = "web"
scheme = "http"
endpoint = "https://raw.githubusercontent.com"
```

For non-AWS S3 endpoints, d2mz enables path-style addressing and disables
config/credential file lookup by default; both can be overridden explicitly.

Every key other than `name` and `scheme` is passed through to the matching
[OpenDAL](https://opendal.apache.org/) service, so any supported service can
be configured the same way.

## Testing

```sh
cargo test                                   # unit + CLI tests (no external deps)
D2MZ_RUSTFS_BIN=... D2MZ_RUSTFS_CLI=... \
  cargo test --features test-support         # + real S3 tests
```

See [`docs/testing-s3.md`](docs/testing-s3.md) for the RustFS setup.

## Design

See [`docs/design.md`](docs/design.md) for the architecture and roadmap.

## License

MIT
