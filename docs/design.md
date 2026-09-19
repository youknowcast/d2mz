# d2mz design

## Goal

A single CLI that browses and archives data across local and remote storage
without the user caring where the bytes physically live.

## Layers

```
CLI (clap)   ls / cat / stat / find / ingest / tag / search
   │
URI          mz://<backend>/<path>;  bare path = local
   │
MZ           config + env → opendal::Operator  (src/backend.rs)
   │
OpenDAL      services-fs, services-s3, ... (sftp, webdav, gcs, ...)
   │
archive      SQLite index + BLAKE3 content-addressed store
```

The **MZ** layer is the core idea: a URI names a configured backend, and
`Backends::resolve` turns it into an OpenDAL `Operator`. Everything above
works against the uniform operator API. The archive is itself just another
backend, so ingesting from a remote source is a copy between two operators.

## URI scheme

| Input                     | Backend | Path          |
| ------------------------- | ------- | ------------- |
| `/var/log/syslog`         | `local` | `var/log/...` |
| `./notes.txt`             | `local` | `notes.txt`   |
| `mz://r2/backups/x.tar`   | `r2`    | `backups/x.tar` |
| `local://etc/hosts`       | `local` | `etc/hosts`   |

## Configuration

Named backends in TOML. `name` and `scheme` are interpreted by d2mz; every
other key is forwarded to the OpenDAL service builder as a string. This keeps
d2mz service-agnostic: adding WebDAV or SFTP needs no code, only config.

```toml
archive_dir = "~/.local/share/d2mz"

[[backend]]
name = "r2"
scheme = "s3"
bucket = "mybucket"
endpoint = "https://<account>.r2.cloudflarestorage.com"
region = "auto"
```

An implicit `local` backend (`fs`, root `/`) always exists.

## Browsing semantics

Object stores are flat. A "directory" is a prefix. `ls` lists immediate
children via `list_with(prefix).recursive(false)`, treats the target as a
prefix (trailing slash), drops the target echoed back by some backends, and
sorts by path. When the target is a file it is shown as a single entry.

## Archive

- Content addressing with **BLAKE3**; blobs sharded as
  `store/<hash[0:2]>/<hash[2:4]>/<hash>`. Bytes are streamed to a temporary
  file while hashing, then renamed into place, so ingest is atomic and
  memory stays flat for large objects.
- SQLite index (`rusqlite`, bundled):

  ```sql
  blob(hash PK, size, created_at)
  entry(id, blob_hash FK, name, path, source, size, created_at)
  ```

  `blob` rows are unique contents; `entry` rows are named sightings. Many
  entries may point at one blob, which is exactly deduplication.

  Tags and metadata hang off entries:

  ```sql
  tag(entry_id FK, tag)          -- PRIMARY KEY (entry_id, tag)
  meta(entry_id FK, key, value)  -- PRIMARY KEY (entry_id, key)
  ```

  Search uses FTS5 over `name`, `path`, `source` and the entry's tags. The
  index is kept in sync on ingest and on every tag change, and `reindex()`
  rebuilds it from scratch when needed.
- Ingest is non-destructive (copy, never move). `export` resolves a hash
  prefix to a blob and writes it back to any backend.
- Tag targets resolve by entry id, exact path/name/source, a path suffix
  (`docs/report.txt`), or a path/source prefix.

### Source presence

A source may vanish and reappear, so entries carry an observed state rather
than being deleted:

```
present --(scan finds nothing)--> missing --(scan finds it again)--> present
   |
   +--(forget)--> deleted  (tombstone; never scanned again)
```

`entry.state` records the observation, `entry.seen_at` when it was last
switched, and `entry.mtime` the source's modification time so `scan` can tell
an unchanged file from a changed one without downloading it. Blobs are never
dropped when a source disappears, so a reappearing file re-registers for
free. Physical removal is a separate concern (purge), not a side effect of
scanning.


- The index opens with WAL mode; SQLite stores sizes as `i64`, converted at
  the boundary.

## Roadmap

- **M0** (done) config, MZ resolver, `ls` / `cat` / `stat` on local files.
- **M1** (done) `find`, recursive listing, name/size filters.
- **M2** (done) S3-compatible backend, verified against RustFS.
- **M3** (done) archive schema, `ingest` / `export`, BLAKE3 dedup.
- **M4** (done) tags and search (SQLite FTS5).
- **M5** (done) static musl build, shell completions, man page.

## Backends

Any OpenDAL service can be added from configuration alone; no d2mz code
changes are needed. Currently compiled in:

| scheme  | use                                            |
| ------- | ---------------------------------------------- |
| `fs`    | local filesystems                              |
| `s3`    | AWS S3, RustFS, MinIO, R2, ...                 |
| `sftp`  | any SSH host (LAN or Tailscale), key auth only |
| `http`  | read-only HTTP(S) objects                      |

SFTP needs `endpoint`, `user`, `key` (a private key path; passwords are not
supported) and optionally `known_hosts_strategy` and `root`. It is verified
against a real `sshd` in `tests/sftp.rs`.

For non-AWS S3 endpoints d2mz defaults to path-style addressing and disables
config/credential file lookup; both are overridable.

## Main database and sync

The archive is usable with no remote at all. When a shared catalogue is
wanted, there is exactly **one** main database, declared once as
`main = "mz://..."` in the config (`--remote` overrides it per invocation).
It stores a portable JSON snapshot of the index, not a live SQLite file,
because object stores cannot host a database that is written in place.

`d2mz sync`:

1. download the remote snapshot (read-only, before locking)
2. acquire a lease-based lock next to it
3. merge remote rows into the local index
4. compute local rows the remote lacks or trails on, and push

Merge rules:

- **entries**: keyed by `source`, last-writer-wins on `updated_at`
- **blobs**: union by hash
- **tags / meta**: set union

Every write stamps `entry.updated_at` and `entry.origin` (the node id), which
is what makes last-writer-wins decidable.

### One main database, enforced

Because a single main database is the source of truth, two mistakes are
worth guarding:

- **Identity.** The snapshot carries a `main_id`, minted on `--init`. Each
  node stores the id it first synced with (`node.main_id`) and refuses to
  merge against a different one. Two catalogues can never be mixed by
  accident.
- **Overwrite.** `--init` on a non-empty main database fails unless
  `--force` is given, so seeding cannot silently discard the catalogue.

### The lease lock

POSIX locks do not exist on S3, so exclusion is a lock object created
atomically (`write_with(..).if_not_exists(true)`, i.e. `If-None-Match: *` on
S3 and rename on a filesystem). Its body is
`{owner, host, acquired_at, expires_at}`. A live lease rejects other writers;
an expired one may be stolen, so a crashed caller cannot block the archive
forever. `LockGuard::release` deletes the object only if this node still owns
it.

IDs are keyed by URI (`mz://<backend>/<path>`), so a shared main database
assumes every node addresses the same source the same way — configure the
same backend names on each machine.

## Static builds

A musl C compiler is unavoidable: `libsqlite3-sys` bundles SQLite and ring
compiles assembly/C. `scripts/build-static.sh` downloads zig and uses it as
that compiler, then links with rust's own `rust-lld` and self-contained musl
CRT (`-C linker=rust-lld -C link-self-contained=yes`). Sharing zig's CRT with
rust's produces duplicate `_start` symbols, so zig is restricted to compiling
C and never links.

The resulting `target/x86_64-unknown-linux-musl/release/d2mz` is a
static-pie binary with no dynamic dependencies.

## Build and dependencies

The release binary is built with `strip`, thin LTO, one codegen unit, and
`panic = "abort"`. TLS uses rustls with the **ring** provider instead of the
default aws-lc-rs, which halves the binary size (14M → 7.5M). Because the
provider is not auto-selected with `rustls-no-provider`, `d2mz::init_http()`
installs the ring provider and the reqwest transport before any HTTP backend
is constructed.
