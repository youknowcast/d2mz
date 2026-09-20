# d2mz

**Door 2 MeriaZard** — one CLI to browse and archive local and remote data
sources.

d2mz puts every backend (local files, S3-compatible object storage, and more
to come) behind a single virtual data source called **MZ**. The same commands
work no matter where the bytes live.

> Status: early. The read-only browse commands (`ls`, `cat`, `stat`) work
> against local files and S3-compatible storage. The content-addressed
> archive is designed but not implemented yet. The original Python prototype
> is preserved under [`legacy/`](legacy/).

## Build

```sh
cargo build --release
# binary at target/release/d2mz
```

## Usage

A target is a URI. Bare paths mean the `local` backend.

```sh
d2mz ls                          # current directory
d2mz ls /var/log                 # absolute local path
d2mz ls -l /var/log              # long listing
d2mz ls --json /var/log          # machine readable
d2mz cat ./notes.txt             # stream a file to stdout
d2mz stat ./notes.txt            # metadata
d2mz stat --json ./notes.txt     # metadata as JSON

d2mz ls mz://r2/backups/2026     # a named remote backend
```

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

[[backend]]
name = "r2"
scheme = "s3"
bucket = "mybucket"
endpoint = "https://<account>.r2.cloudflarestorage.com"
region = "auto"
# credentials are read from the environment (AWS_ACCESS_KEY_ID, ...)
```

Every key other than `name` and `scheme` is passed through to the matching
[OpenDAL](https://opendal.apache.org/) service, so any supported service can
be configured the same way.

## Design

See [`docs/design.md`](docs/design.md) for the architecture and roadmap.

## License

MIT
