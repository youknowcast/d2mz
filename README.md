# d2mz

**Door 2 MeriaZard** — one CLI to browse and archive local and remote data
sources.

d2mz puts every backend (local files, S3-compatible object storage, and more
to come) behind a single virtual data source called **MZ**. The same commands
work no matter where the bytes live.

> Status: early. Browse (`ls`, `cat`, `stat`, `find`) works against local
> files, S3-compatible storage, and plain HTTP(S) URLs. A content-addressed
> archive (`ingest`, `archive`, `export`) performs BLAKE3-based
> deduplication, with tags and full-text search (`tag`, `search`). The
> original Python prototype is preserved under [`legacy/`](legacy/).

## Layout

The workspace separates the layers so the dependency direction is one-way:

| Crate | Role |
| ----- | ---- |
| `crates/d2mz` | CLI (clap), commands, output, application config |
| `crates/mz` | URI resolution, backend config, prefix listing (OpenDAL) |
| `crates/d2mz-archive` | SQLite index, BLAKE3 blob store, sync, thumbnails |

`d2mz` depends on `mz` and `d2mz-archive`; those two depend only on OpenDAL
and not on each other or on the CLI.

## Build

```sh
cargo build --release
# dynamic glibc binary at target/release/d2mz

scripts/build-static.sh                 # fully static musl binary via zig
scripts/install.sh                      # build static + install binary/man/completions
scripts/install.sh --prefix /usr/local  # install elsewhere
```

The static build needs a musl C compiler because bundled SQLite and ring
compile C; the script downloads zig to provide one without touching system
packages.

`d2mz --version` reports the crate version plus the build's git revision,
for example `0.8.0+bf2ac36`, so it is clear which working tree a binary came
from. Non-git builds omit the suffix.

## Shell integration

`scripts/install.sh` installs the man page and bash completion for you. To
do it by hand:

```sh
d2mz completions bash > ~/.local/share/bash-completion/completions/d2mz
d2mz completions zsh  > ~/.zfunc/_d2mz
d2mz completions fish > ~/.config/fish/completions/d2mz.fish
d2mz man > /usr/local/share/man/man1/d2mz.1   # roff
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
```

### Archive

`ingest` copies objects into a local content-addressed store: identical
contents are written once and shared between entries. `export` writes a blob
back out to any backend. Tags and full-text search run on the same SQLite
index.

Registering is explicit and per-source; **searching is not**. `search` and
`archive` never contact a backend — they query the local index, so they span
every source you have ingested and need no URI:

```sh
d2mz ingest -R ./photos                    # register a local tree
d2mz ingest mz://rfs/backups --name '*.tar'

d2mz scan                                   # re-check sources: present/missing
d2mz scan mz://rfs/backups                  # only one source subtree
d2mz archive --state missing -l             # what disappeared
d2mz forget --id old-report.txt             # retire an entry (tombstone)

d2mz search report                          # across all ingested entries
d2mz search 'work OR holiday' -l            # name/path/source/tags only
d2mz archive -l                             # list everything ingested
d2mz archive --prefix photos/ --json

d2mz tag add holiday --id photos/beach.jpg # by path, name or source URI
d2mz tag add work --prefix docs/           # by path/source prefix
d2mz tag list holidays                      # entries carrying a tag
d2mz tag list                               # all tags with counts
d2mz tag rm work --id docs/report.txt

d2mz export 3f9a1c2b ./restored.jpg        # hash prefix is enough
d2mz export 3f9a1c2b mz://rfs/restored.jpg
```

### Opening files

`open` picks an application by file kind (`--as` overrides detection) and
runs it. Handlers are stored in the index and shared through `sync`, so every
machine opens the same types the same way.

```sh
d2mz open ./notes.md                  # text -> $PAGER
d2mz open ./photo.png                 # image/video -> handler or xdg-open
d2mz open --with 'imv {}' ./a.jpg     # this once
d2mz open --print ./a.jpg             # just the local path

d2mz handler set image --app 'imv {}'
d2mz handler set image --matcher .svg --app 'inkscape {}'
d2mz handler list
d2mz handler rm image --matcher .svg
```

Directories are listed, executables are never run. Remote objects are
materialised into a temporary file; remote text is piped to the pager.

### Thumbnails

Media thumbnails are generated once, keyed by the blob's BLAKE3 hash, and
cached under `archive_dir/thumb/` — identical contents share one thumbnail,
and **no network access happens after the first read**.

`ingest` generates them by default while the bytes are already in hand, so
there is no extra fetch. Pass `--no-thumb` to skip, and use `scan --thumb`
later to backfill.

```sh
d2mz ingest -R ./photos                 # thumbnails made as part of ingest
d2mz ingest --no-thumb ./big-dump       # skip for a bulk import
d2mz scan --thumb                       # backfill existing media

d2mz thumb ./photo.png                  # render in the terminal (if a viewer exists)
d2mz thumb -l ./photo.png               # also print dimensions
d2mz thumb --print ./photo.png          # just the cached path
```

Images are resized in-process (`image` crate). Videos use `ffmpeg` for the
first frame when it is installed; otherwise they are skipped. Terminal
rendering uses `chafa`, `viu`, `tiv` or `img2txt` if one is present, and
otherwise just reports the path.

### Source presence

Sources come and go. `scan` re-checks every registered source and records
whether it is still there:

- `present` — confirmed by the latest scan
- `missing` — absent at the latest scan; may reappear
- `deleted` — retired with `forget`, never scanned again

Missing entries keep their blob, so a file that disappears and comes back
costs nothing to re-register. `scan` also detects changed contents (by mtime
and size) and stores the new version alongside the old one. Default listings
mark state: a leading `!` means missing, `x` means deleted.

### Remote main database (optional)

d2mz works entirely offline: without a remote it is just the local index.
There is exactly **one** main database shared by every machine; declare it
once in the config and `sync` needs no arguments.

```toml
# ~/.config/d2mz/config.toml
main = "mz://rfs/d2mz/main.db"     # or mz://mac/Users/ada/d2mz/main.db
```

```sh
# On the first machine: create the main database from the local index.
d2mz sync --init

# On every machine: pull remote changes and push local ones.
d2mz sync
```

`--remote` still overrides the configured `main`. Merging is last-writer-wins
per entry (`updated_at`), with tags and metadata unioned. Writers are
serialised by a lease-based lock next to the database (`main.lock.json`); a
crashed holder is recovered once its lease expires.

Safety rails, because there is only one main database:

- The main database carries an id. A node remembers the id it first synced
  with, so pointing at a **different** main database is an error rather than
  a silent merge of two catalogues.
- `--init` refuses to overwrite an existing main database; pass `--force` to
  say so explicitly.

Default listings show the backend, size, hash prefix and path, so you can
tell at a glance which source a hit came from.

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

# Any SSH host on the LAN or over Tailscale, via its SFTP subsystem.
[[backend]]
name = "mac"
scheme = "sftp"
endpoint = "ssh://mac.local:22"       # or the Tailscale name / IP
user = "ada"
key = "~/.ssh/id_ed25519"             # key-based auth only
known_hosts_strategy = "accept"
root = "/Users/ada"
```

For non-AWS S3 endpoints, d2mz enables path-style addressing and disables
config/credential file lookup by default; both can be overridden explicitly.

For SFTP, `endpoint` accepts `[user@]host[:port]` or
`ssh://[user@]host[:port]`, `key` is a private key path (passwords are not
supported), and `known_hosts_strategy` is `strict` (default), `accept` or
`add`. This works the same over Tailscale since it is ordinary SSH.

Every key other than `name` and `scheme` is passed through to the matching
[OpenDAL](https://opendal.apache.org/) service, so any supported service can
be configured the same way.

## Testing

```sh
cargo test                                   # unit + CLI tests (no external deps)
D2MZ_RUSTFS_BIN=... D2MZ_RUSTFS_CLI=... \
  cargo test --features test-support         # + real S3 and SFTP tests
```

The SFTP tests use the system `sshd` and are skipped when it is missing
(override with `D2MZ_SSHD_BIN`). See
[`docs/testing-s3.md`](docs/testing-s3.md) for the RustFS setup.

## Design

See [`docs/design.md`](docs/design.md) for the architecture and roadmap.

## License

MIT
