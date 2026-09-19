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
  entries may point at one blob, which is exactly deduplication. Tags and
  metadata tables (`tag`, `meta`) arrive in M4.
- Ingest is non-destructive (copy, never move). `export` resolves a hash
  prefix to a blob and writes it back to any backend.
- The index opens with WAL mode; SQLite stores sizes as `i64`, converted at
  the boundary.

## Roadmap

- **M0** (done) config, MZ resolver, `ls` / `cat` / `stat` on local files.
- **M1** (done) `find`, recursive listing, name/size filters.
- **M2** (done) S3-compatible backend, verified against RustFS.
- **M3** (done) archive schema, `ingest` / `export`, BLAKE3 dedup.
- **M4** tags and search (SQLite FTS5).
- **M5** static musl build, shell completions, man page.

## Build and dependencies

The release binary is built with `strip`, thin LTO, one codegen unit, and
`panic = "abort"`. TLS uses rustls with the **ring** provider instead of the
default aws-lc-rs, which halves the binary size (14M → 7.5M). Because the
provider is not auto-selected with `rustls-no-provider`, `d2mz::init_http()`
installs the ring provider and the reqwest transport before any HTTP backend
is constructed.
