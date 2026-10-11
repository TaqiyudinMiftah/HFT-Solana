# Windows portability patch for Yellowstone client 14.0.1

Source: published `yellowstone-grpc-client` **14.0.1** from crates.io.
The normalized manifest, four Rust source files, upstream README, license,
and VCS metadata are preserved. See `.cargo_vcs_info.json` for the upstream
commit and `LICENSE_APACHE2` for the Apache-2.0 license.

The published crate imports `tokio::net::UnixStream` unconditionally and
defines `GeyserGrpcBuilder::connect_uds` on every platform. Tokio only provides
this type on Unix. Native Windows compilation fails with `E0432`, even when
the application uses ordinary TCP gRPC and never calls `connect_uds`.

The entire functional patch is in `src/lib.rs`:

1. Move `std::path::PathBuf` and `tokio::net::UnixStream` into a `#[cfg(unix)]`
   import.
2. Mark `connect_uds` with `#[cfg(unix)]`.
3. Import `Uri` only for Unix or the `test-tools` connector APIs, avoiding an
   unused import on Windows while preserving custom-connector support.

TCP/TLS transport, protobuf versions, reconnect, deduplication, and all quote
math remain the published implementation. Unix retains its UDS API. Windows
has the TCP/TLS API used by HFT-Solana and does not expose a UDS method.

Do not edit the global Cargo cache to reproduce this fix. The root
`[patch.crates-io]` entry selects this source and `Cargo.lock` records it.
Remove this override after a compatible upstream release supplies the same
platform guard and passes the project's regression suite. Version 15.0.0
and upstream master still had the unconditional import when inspected on
2026-10-11; upgrading the protocol was not required for this fix.

Verification commands, from the HFT-Solana root:

```text
cargo fmt --all -- --check
cargo test --all-targets --all-features
cargo build --release --features yellowstone --bin paper_yellowstone
```

For a source comparison with the downloaded published crate, use
`git diff --no-index <registry-source>/src/lib.rs vendor/yellowstone-grpc-client/src/lib.rs`.
The other three source files must remain byte-identical to 14.0.1.
