# Local setup: Windows PowerShell, paper mode

Pemeriksaan lokal dimulai pada 11 Oktober 2026. Tidak ada signer, private key,
pengiriman transaksi, atau pembayaran fee dalam langkah di bawah.

## Checkout dan mesin yang diperiksa

- Windows 11 Home Single Language, build `10.0.26100`, x64.
- Windows PowerShell `5.1.26100.7019` (Desktop), bukan WSL/Bash.
- AMD Ryzen 5 7640HS, 6 core / 12 logical processors, RAM sekitar 23.2 GiB.
- Workspace: `C:\Adn\Adn Belajar python\HFT-Solana`.
- Git `2.45.2.windows.1`; `protoc 25.3` sudah tersedia dari Anaconda.
- Checkout awal: `main`, HEAD `9457cccd094e47fc66f9b0da5c8f9284c482388d`, bersih.
- Setelah `git fetch origin` dan `git switch dev/hft-v1`: HEAD lokal dan remote
  sama, `cc1426920fb0fb9ac1e6649a7729bd5fd5e51da7`, working tree awal bersih.
- [PR #1](https://github.com/TaqiyudinMiftah/HFT-Solana/pull/1) sudah **merged**,
  bukan draft terbuka seperti dalam handoff lama.
- [CI #249](https://github.com/TaqiyudinMiftah/HFT-Solana/actions/runs/37182240205)
  sukses pada HEAD tersebut. Workflow menggunakan Ubuntu; hasilnya tidak
  menggantikan pengujian native Windows.
- HTTP HEAD ke `index.crates.io/config.json`, manifest stable Rust, dan installer
  Rust sukses. `git ls-remote` kedua repository Meteora juga sukses; pinned
  revisions dalam `Cargo.toml` cocok dengan HEAD upstream saat pemeriksaan.

## Hasil lokal

Rust `1.99.0 (b940084d7 2026-09-28)`, Cargo `1.99.0
(5f94df478 2026-08-27)`, host `x86_64-pc-windows-msvc`, LLVM `23.1.1`.
`cargo fmt --all -- --check` lulus (exit 0).

Percobaan test pertama gagal (exit 101, 84.64 detik) pada build script `ahash`:

```text
error: linking with `link.exe` failed: exit code: 1181
LINK : fatal error LNK1181: cannot open input file 'dbghelp.lib'
error: could not compile `ahash` (build script) due to 1 previous error
```

MSVC sudah tersedia, tetapi installer Windows SDK masih berjalan. Mitigasi:
tunggu installer selesai dan verifikasi library SDK sebelum mengulang test;
tidak ada patch Rust yang dibutuhkan untuk error ini. Visual Studio Build Tools
2022 `17.14.41` (`17.14.37710.0`) kemudian selesai dengan exit 0, complete,
tanpa reboot requirement. MSVC `14.44.35207` dan Windows SDK `10.0.26100.0`
terdeteksi; `um/x64/DbgHelp.Lib` tersedia. Log percobaan disimpan
sebagai `target/local-setup/tests-toolchain.log`.

Percobaan kedua gagal (exit 101, 64.55 detik) karena `openssl-sys 0.9.117`
tidak menemukan OpenSSL development headers/libraries. `cargo tree --locked
--all-features -i openssl-sys` menunjukkan dependency ini diperlukan oleh
`solana-secp256r1-program 2.2.4` melalui Solana SDK/precompiles.

OpenSSL **3.0.19** sudah terpasang di Anaconda pada mesin ini, termasuk headers,
`libssl.lib`, `libcrypto.lib`, dan DLL x64. Mitigasi memakai `OPENSSL_DIR` dan
dynamic linking, bukan patch dependency. Log ada di
`target/local-setup/tests-openssl.log`.

Percobaan ketiga (exit 101, 83.97 detik) menemukan incompatibility OS pada
`yellowstone-grpc-client 14.0.1`:

```text
error[E0432]: unresolved import `tokio::net::UnixStream`
yellowstone-grpc-client-14.0.1/src/lib.rs:22:5
error: could not compile `yellowstone-grpc-client` (lib) due to 1 previous error
```

Ini bug portability dependency, bukan Rust/MSVC yang hilang. Published client
15.0.0 dan upstream master juga masih mengimpor tipe itu tanpa Unix guard.
Root `Cargo.toml` kini memakai `[patch.crates-io]` ke salinan client **14.0.1**
di `vendor/yellowstone-grpc-client`. Perubahan fungsional hanya mengaktifkan
import Unix socket dan metode `connect_uds` pada `#[cfg(unix)]`; TCP, protobuf,
reconnect, dan DEX math tidak diubah. License/provenance disertakan; tiga file
source lain sudah dibandingkan byte-for-byte dengan crate asli. Lihat
[patch notes](../vendor/yellowstone-grpc-client/PATCH.md). Log gagal disimpan
di `target/local-setup/tests-yellowstone-windows.log`.

Setelah patch dan penyesuaian guard import `Uri`, format check lulus dan
`cargo test --all-targets --all-features` lulus (exit 0): **105 tests passed,
0 failed, 0 ignored**, 38 test targets termasuk target berisi 0 tests.
Percobaan akhir memakai cache dari kompilasi sebelumnya; wall time 32.52
detik, Cargo melaporkan 26.53 detik untuk build test profile. Tidak ada
warning pada percobaan akhir. Profile `test` unoptimized, debug symbols 0,
incremental off, 4 build jobs; ini bukan hasil benchmark search latency.

Coverage yang dijalankan mencakup Pump/Raydium integer parity, DAMM/DLMM
official-SDK fixtures, stale/fork rollback, snapshot generations, static
dependency causal-version updates, bounded dirty queue/coalescing, DLMM fences/bin coverage,
bootstrap catch-up, config topology, dan paper landing/calibration importer.
`cargo build --release --features yellowstone --bin paper_yellowstone` juga
lulus (exit 0): **277.89 detik** wall time, Cargo melaporkan **4m 37s**,
profile `release` optimized, target `x86_64-pc-windows-msvc`, 4 build jobs,
tanpa warning. Build ulang dengan `--locked` juga lulus memakai cache.

Artifact yang benar-benar dibuat:

```text
target/release/paper_yellowstone.exe
size: 8,597,504 bytes (about 8.20 MiB)
SHA-256: e4182449c92033f0e0f2c67d9a7f7bcaf00467d2358ce124a072f56d130504df
```

Binary dapat dimuat pada Windows lokal. Smoke checks offline berikut
memperoleh exit **1 yang diharapkan**, sebelum startup subscription:

```text
paper_yellowstone.exe
Error: Custom { kind: InvalidInput, error: "usage: paper_yellowstone <config.json>" }

paper_yellowstone.exe config/paper.local.json
Error: InvalidPubkey { field: "tokens[]", value: "REPLACE_WITH_QUOTE_MINT" }
```

Log ini hasil eksekusi aktual dan tidak berisi credential. Tidak ada
subscription yang dicoba. Ini adalah smoke check pada template awal, sebelum
config lokal diisi dengan account nyata pada pemeriksaan lanjutan di bawah.

`dumpbin /dependents` pada release executable menunjukkan Windows system DLLs
dan `VCRUNTIME140.dll`, tanpa import langsung DLL OpenSSL. Smoke check dengan
Anaconda dihapus dari PATH child process juga lulus. OpenSSL development
files tetap diperlukan untuk kompilasi dependency; tidak berarti release
executable ini memakai OpenSSL ketika dimuat. Disk C: tersisa sekitar 4.2 GiB
setelah test dan release build; artifact debug/release tetap disimpan.

**Pemeriksaan lanjutan lulus:** bootstrap RPC satu kali dengan config nyata,
sembilan dependency accounts, kedua pool quote-ready, dan dua arah quote
WSOL/USDC. Ini memakai example `paper_bootstrap` tanpa Yellowstone; hasil
terperinci dan reproduksi ada di bagian 4.

**Belum diuji:** Yellowstone feed nyata, reconnect provider, slot fences dengan
live data, streaming opportunity/landing output, runtime berkelanjutan dengan
config valid, dan execution pada WSL/Linux/macOS.
CLI calibration tidak dijalankan karena tidak ada labeled-outcome dataset.
CI Ubuntu #249 berlaku untuk HEAD sebelum patch lokal ini, bukan bukti patch
baru sudah diuji di Ubuntu.
Log build/test lokal disimpan dalam `target/local-setup/` (di-ignore Git).

`Cargo.lock` sebelumnya tidak ada. Cargo menghasilkan 854 package entries
(853 dependencies ditambah aplikasi), termasuk dua pinned Meteora revisions
dan transitive git pin `anza-xyz/pinocchio` pada
`17b0e862c01a868ea07ef81a2f8a9b4a504bdfed`. Lockfile disertakan sebagai
perubahan lokal yang perlu direview dan di-commit bersama aplikasi.

Saat inspeksi awal, `rustup`, `rustc`, dan `cargo` belum terpasang. Visual Studio
Community 2022 yang terdeteksi berstatus incomplete dan tidak memiliki komponen
`Microsoft.VisualStudio.Component.VC.Tools.x86.x64`. Ini masalah toolchain lokal,
bukan bukti bug Rust proyek.

## 1. Prasyarat native Windows

Panduan resmi:
[Rust Windows installation](https://rust-lang.org/tools/install/),
[MSVC prerequisites](https://rust-lang.github.io/rustup/installation/windows-msvc.html),
[Visual Studio installer CLI](https://learn.microsoft.com/en-us/visualstudio/install/use-command-line-parameters-to-install-visual-studio?view=vs-2022).

Jika Rust belum tersedia, unduh installer resmi dan jalankan:

```powershell
$installerDir = Join-Path $env:TEMP 'hft-local-setup'
New-Item -ItemType Directory -Path $installerDir -Force | Out-Null
$installer = Join-Path $installerDir 'rustup-init.exe'
Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile $installer -UseBasicParsing
& $installer -y --profile minimal --default-host x86_64-pc-windows-msvc --default-toolchain stable --component rustfmt --no-modify-path
```

Pertahankan nama `rustup-init.exe`: installer Rust memilih mode dari nama file;
nama arbitrer dapat menghasilkan `unknown proxy name`.

Jika komponen MSVC/SDK belum tersedia, perintah yang digunakan dalam setup ini:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --source winget --accept-source-agreements --accept-package-agreements --silent --override '--quiet --wait --norestart --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --addProductLang en-US'
```

Instalasi MSVC/SDK dapat meminta elevasi Windows dan mengunduh beberapa GB.
Periksa setelah instalasi selesai:

```powershell
& 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe' -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format json
$env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
rustup show
rustc -Vv
cargo --version
```

`--no-modify-path` dipakai agar installer tidak mengubah PATH permanen. Jalankan
penambahan PATH di setiap PowerShell baru. Rust dapat menemukan MSVC lewat
instalasi Visual Studio meskipun `cl.exe` tidak ada di PATH biasa.

## 2. Sinkronkan checkout tanpa menghapus perubahan lokal

Untuk checkout yang sudah ada:

```powershell
Set-Location 'C:\Adn\Adn Belajar python\HFT-Solana'
git status --short --branch
git fetch origin
git switch dev/hft-v1
git rev-parse HEAD
git rev-parse origin/dev/hft-v1
```

Jika kedua SHA berbeda, periksa riwayat dan perubahan lokal dahulu. Jangan
gunakan `reset --hard` atau force-push. Untuk mesin baru, jalankan `git clone
https://github.com/TaqiyudinMiftah/HFT-Solana.git` di direktori induk, lalu masuk
ke checkout tersebut.

## 3. Format, test, build

Disk C: awalnya tersisa sekitar 16.3 GiB. Untuk mengurangi artifact debug pada
dependency graph Solana yang besar, gunakan environment berikut dalam terminal
yang sama. Debug assertions pada test tetap aktif; yang dinonaktifkan adalah
debug symbols dan incremental compilation.

```powershell
$env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_BUILD_JOBS = '4'
# Mesin ini memakai development libraries OpenSSL yang sudah ada di Anaconda.
$env:OPENSSL_DIR = 'C:\Users\taqiy\Anaconda3\Library'
$env:OPENSSL_STATIC = '0'
$env:PATH = (Join-Path $env:OPENSSL_DIR 'bin') + ';' + $env:PATH
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw 'Format check failed' }
cargo test --all-targets --all-features
if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
cargo build --release --features yellowstone --bin paper_yellowstone
if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
Get-Item .\target\release\paper_yellowstone.exe
```

PowerShell 5.1 tidak mendukung operator `&&`; jalankan perintah berurutan dan
periksa `$LASTEXITCODE`. Untuk build berikutnya setelah `Cargo.lock` tersedia,
tambahkan `--locked` pada perintah test/build agar dependency graph tidak
berubah diam-diam. Lockfile aplikasi sebaiknya disertakan dalam PR.

Untuk PC tanpa Anaconda, siapkan OpenSSL x64 development headers dan libraries
yang kompatibel dengan MSVC, lalu sesuaikan `OPENSSL_DIR`. Jika binary/test
merujuk DLL OpenSSL, DLL dynamic tersebut harus ada di PATH. Release artifact
setup ini tidak mengimpor DLL OpenSSL secara langsung, tetapi tetap memakai
Windows system/MSVC runtime DLLs.
Lihat [manual OpenSSL discovery](https://docs.rs/openssl/latest/openssl/#manual).

## 4. Config lokal dan validasi

```powershell
if (-not (Test-Path -LiteralPath 'config/paper.local.json')) {
    Copy-Item config/paper.example.json config/paper.local.json
}
git check-ignore -v config/paper.local.json
Select-String -Path config/paper.local.json -Pattern 'REPLACE_WITH_'
```

`config/paper.local.json` sudah dikecualikan lewat `.gitignore`. Jangan gunakan
`git add -f` untuk file ini. Jangan letakkan endpoint/token di config atau
dokumentasi.

Template saat diperiksa memiliki **16 kemunculan, 12 placeholder unik**:

| Placeholder | Data yang harus diverifikasi melalui RPC |
| --- | --- |
| `QUOTE_MINT`, `BASE_MINT` | Mint yang sama pada kedua pool; owner, decimals, extensions |
| `PUMP_POOL` | PumpSwap pool dan mint/vault yang tersimpan di account |
| `PUMP_FEE_CONFIG` | Account konfigurasi fee yang cocok dengan program |
| `PUMP_GLOBAL`, `PUMP_AMM_GLOBAL_CONFIG` | Account global milik program yang benar |
| `PUMP_BASE_VAULT`, `PUMP_QUOTE_VAULT` | Token accounts dengan mint dan authority yang cocok |
| `RAYDIUM_POOL`, `RAYDIUM_AMM_CONFIG` | Raydium **CPMM**, bukan AMM v4/CLMM |
| `RAYDIUM_VAULT_0`, `RAYDIUM_VAULT_1` | Vault berdasarkan urutan mint dalam account pool |

Semua nama pada tabel memiliki awalan `REPLACE_WITH_`. Tidak ada alamat baru
yang dikarang atau diambil dari synthetic test fixtures.

Pemeriksaan JSON lokal memastikan topology **secara simbolik**:

- Token 0 = quote; token 1 = base.
- Pool 0 Pump: A = base, B = quote; `b_to_a` berarti quote -> base.
- Pool 1 Raydium: A = `mint_0` = base, B = `mint_1` = quote;
  `a_to_b` berarti base -> quote.
- Cycle `[0 -> 1 -> 0]` berisi dua leg, contiguous, dan kembali ke start token.
- Nilai search adalah JSON integer, bukan bilangan pecahan:

| Field | Raw value | Jika start mint terbukti WSOL, decimals = 9 |
| --- | ---: | ---: |
| `initial_seed` | 10000000 | 0.01 SOL |
| `minimum_probe` | 1000000 | 0.001 SOL |
| `max_size` | 1000000000 | 1 SOL |
| `minimum_effective_profit` | 1000 | 0.000001 SOL |

Kolom SOL hanya ilustrasi unit; mint template belum nyata dan decimals belum
diverifikasi. Mengganti placeholder dapat mengubah urutan mint; periksa lagi
seluruh edge setelah account RPC tersedia.

Template tetap **belum valid untuk live data**. Binary memanggil
`PaperConfig::build` sebelum menghubungkan feed, sehingga placeholder harus
ditolak. Config loader memeriksa key dan topology; identitas/owner/data account
nyata tetap memerlukan RPC dan decoder. Probability/fee contoh adalah
demonstrasi, bukan kalibrasi.

Pemeriksaan lanjutan memakai dua pool WSOL/USDC nyata dari indeks resmi
[Raydium](https://api-v3.raydium.io/docs/) dan
[Meteora DAMM v2](https://docs.meteora.ag/api-reference/damm-v2/pools/pools).
USDC mint dicocokkan dengan
[daftar resmi Circle](https://developers.circle.com/stablecoins/usdc-contract-addresses).
API indeks dipakai hanya untuk discovery; account RPC menjadi sumber validasi.

| Pool ID lokal | DEX | Pool account | Program owner |
| --- | --- | --- | --- |
| 0 | Raydium CPMM | `47hq28mcL7q5GhBg7epyGF2dnuJd4MKFt8QhT7CzYUp4` | `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C` |
| 1 | Meteora DAMM v2 | `8Pm2kZpnxD3hoMmt4bjStX2Pw2Z9abpbHzZxMPqxPmie` | `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG` |

`getMultipleAccounts` membaca kesembilan account dependency dalam satu response
processed, context slot `455507469`. Owner program, discriminator, pasangan
mint, vault mint, status token account, serta mint initialized telah diperiksa.
Pool account berukuran 637 byte (CPMM) dan 1112 byte (DAMM v2); kedua mint
memakai SPL Token legacy (82 byte), WSOL decimals 9 dan USDC decimals 6.
Raydium swap tidak disabled dan DAMM pool status 0. Snapshot publik tanpa
credential ada di `target/local-setup/validated-pool-accounts.json`.

Config lokal sebelumnya terbukti sama dengan template sebelum diganti. Kini
`config/paper.local.json` berisi kedua recipe hasil pembacaan RPC, tanpa
placeholder dan tanpa endpoint/token. Token 0 = WSOL, token 1 = USDC. Kedua
pool menyimpan WSOL sebagai A dan USDC sebagai B, sehingga cycle 0 adalah
CPMM `a_to_b` lalu DAMM `b_to_a`, dan cycle 1 kebalikannya. Keduanya kembali ke
WSOL. Batas paper diperkecil karena CPMM kandidat memiliki cadangan kecil;
ini sampel validasi dan belum merupakan pilihan pool production.

| Field lokal | Raw WSOL units | SOL |
| --- | ---: | ---: |
| `initial_seed` | 1000000 | 0.001 |
| `minimum_probe` | 100000 | 0.0001 |
| `max_size` | 10000000 | 0.01 |
| `minimum_effective_profit` | 1000 | 0.000001 |

Landing candidates tetap parameter contoh. Angka quote/opportunity adalah
prediksi paper, bukan transaksi yang disimulasikan atau profit terealisasi.

Untuk menjalankan ulang dari native Windows PowerShell, gunakan environment
build dari bagian 3, lalu:

```powershell
. .\scripts\Import-PaperProvider.ps1
cargo run --release --locked --features yellowstone --example paper_bootstrap -- config/paper.local.json
```

Perintah tersebut benar-benar dijalankan: exit 0, wall time 1.58 detik,
context slot `455509105`. Output aktual tersanitasi:

```text
PAPER_BOOTSTRAP context_slot=455509105 explicit_accounts=9 initial_opportunities=0
PAPER_POOL_READY pool=0
PAPER_POOL_READY pool=1
PAPER_QUOTE cycle=0 amount_in=100000 amount_out=99846 effective_profit=-154
PAPER_QUOTE cycle=1 amount_in=100000 amount_out=99446 effective_profit=-554
Paper snapshot check finished; no transactions submitted.
```

Quote negatif pada snapshot ini bukan error bootstrap. Keduanya tidak lolos
profitability filter. Reserves, fees, dan hasil quote dapat berubah pada run
berikutnya. Waktu quote mengikuti clock lokal seperti runtime saat ini.

Pemeriksaan ulang pada 11 Oktober 2026 menggunakan executable yang sudah
dibangun, tanpa kompilasi tambahan:

```powershell
. .\scripts\Import-PaperProvider.ps1
.\target\release\examples\paper_bootstrap.exe config\paper.local.json
```

Exit 0, durasi eksekusi binary sekitar 0.20 detik. Snapshot terbaru:

```text
PAPER_BOOTSTRAP context_slot=455510664 explicit_accounts=9 initial_opportunities=0
PAPER_POOL_READY pool=0
PAPER_POOL_READY pool=1
PAPER_QUOTE cycle=0 amount_in=100000 amount_out=99420 effective_profit=-580
PAPER_QUOTE cycle=1 amount_in=100000 amount_out=99863 effective_profit=-137
Paper snapshot check finished; no transactions submitted.
```

Log ada di `target/local-setup/paper-bootstrap-followup.log`. Tidak ada
opportunity yang lolos, sehingga landing policy tidak menghasilkan output
untuk opportunity pada pemeriksaan ini. Ini pembacaan RPC satu kali dan quote
lokal; subscription berkelanjutan, reconnect, serta live slot fences belum
teruji. Credential tidak dicetak dan tidak masuk log.

Setelah perbaikan slot fence, format check lulus; seluruh 105 tests lulus
(40.05 detik, tanpa warning), termasuk regression yang sebelumnya gagal.
Build release `paper_yellowstone` dan example `paper_bootstrap` lulus
(22.70 detik, optimized, tanpa warning). Log ada di
`target/local-setup/fmt-pools.log`, `tests-pools.log`, `build-pools.log`,
`slot-fence-before-fix.log`, dan `paper-bootstrap-live-rpc.log`.

Discovery DAMM via `getProgramAccounts` terkena HTTP 429 tiga kali dengan
pesan `Your app has exceeded its compute units per second capacity`.
Tidak ada retry loop tanpa batas atau upgrade paket. Untuk pasangan ini,
indeks resmi dan bootstrap `getMultipleAccounts` mencukupi; scoped discovery
DLMM tetap memerlukan provider/throughput yang mendukungnya.

Landing policy memakai biaya lamport. Jika policy diaktifkan, gunakan cycle
dengan start asset WSOL agar unit profit dan biaya cocok; untuk start asset
lain, jangan mengurangi biaya lamport dari raw token units tanpa valuasi yang
valid (hapus `landing` untuk observasi quote saja). Cashback lintas token yang
belum dikonversi tidak boleh dihitung sebagai profit start asset.

## 5. Kebutuhan minimum sebelum runtime data nyata

RPC Alchemy Solana Mainnet sudah diperiksa dengan API key yang disediakan
pemilik pada 11 Oktober 2026. Key dan URL lengkap tidak disimpan di repository.
Pemeriksaan read-only berikut berhasil:

| Metode | Hasil pengamatan |
| --- | --- |
| `getSlot`, processed | Slot `455504409` |
| `getGenesisHash` | Mainnet-beta: `5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d` |
| `getAccountInfo`, base64/processed | Account contoh ada, context slot `455504412` |
| `getMultipleAccounts`, base64/processed | Satu account terbaca, context slot `455504760` |

Account contoh `GwsPP9HHhCvEQeu3HTFzsVL6DEtnnYw4ALEtA3fMBC9Q` memiliki
owner System Program (`11111111111111111111111111111111`), data length `0`,
dan `executable=false`. Account ini tidak membuktikan identitas pool DEX dan
tidak dipakai mengganti placeholder config. Jumlah lamport yang terbaca adalah
metadata saldo account, bukan profit bot. `getProgramAccounts` untuk scoped
DLMM belum diuji.

Percobaan batch pertama mengembalikan `-32602` karena array pubkey pada
request pengujian lokal terbungkus satu lapis berlebih. Request diperbaiki
menjadi `params: [[address], config]`; pengujian ulang berhasil. Ini bukan
error decoder Rust atau bukti RPC provider tidak mendukung metode tersebut.

Probe gRPC Python (paket `grpc` sudah tersedia) memakai TLS dan header
`x-token`, dengan deadline 15 detik untuk unary dan 8 detik untuk subscription
slot saja. Kedua probe gagal pada
`https://solana-mainnet.streaming.alchemy.com`:

```text
gRPC GetVersion: UNIMPLEMENTED: /geyser.Geyser/GetVersion not found.
gRPC Subscribe slots: UNIMPLEMENTED: /geyser.Geyser/Subscribe not found.
received slot events=0, received application payload bytes=0
```

Hasil ini membuktikan endpoint tersebut belum bisa dipakai melalui koneksi
yang diuji. Ini tidak membuktikan API key salah, paket team tertentu, atau
kompatibilitas penuh runtime. Konfirmasi endpoint, akses layanan, dan
kompatibilitas protokol pada dashboard/support sebelum berlangganan atau
mengubah runtime. Pemilik mengonfirmasi paket team **Free**.

Pemeriksaan ulang pada 11 Oktober 2026 tetap memperoleh `UNIMPLEMENTED` pada
kedua metode di atas, tanpa update atau application payload. Laporan tersanitasi
ada di `target/local-setup/yellowstone-followup.json`.

Informasi resmi mengenai syarat paket belum konsisten:
[Quickstart Alchemy](https://www.alchemy.com/docs/reference/yellowstone-grpc-quickstart)
menyebut PAYG/Enterprise sebagai prasyarat, sedangkan
[halaman produk gRPC Alchemy](https://www.alchemy.com/solana-grpc) menyatakan
tidak perlu upgrade paket dan biaya berdasarkan bandwidth. Karena itu,
status Free belum membuktikan penyebab error `method not found` ini.
Periksa endpoint gRPC yang tersedia untuk aplikasi Solana di dashboard;
konfirmasikan aktivasi layanan dan dukungan metode Yellowstone bila endpoint
tersebut masih ditolak. Tidak ada upgrade atau perubahan billing yang dilakukan.
Upgrade saja juga belum menjamin kontrak slot/bank yang dibutuhkan runtime.
Paper runtime berkelanjutan belum dijalankan dengan provider nyata.

Pemeriksaan lanjutan tanpa proxy: `GetVersion` tanpa token memperoleh
`UNAUTHENTICATED`, sedangkan dengan token tersimpan memperoleh `UNIMPLEMENTED`.
Ini mendukung diagnosis bahwa token dikenali, tetapi metode tidak tersedia
melalui akses yang diuji. Pemilik kemudian memeriksa dashboard dan mengonfirmasi
bahwa **akses gRPC pada akun ini memerlukan upgrade, tidak tersedia pada paket
Free**. Screenshot memperlihatkan endpoint HTTPS RPC; URL Yellowstone aktif
belum tersedia. Screenshot dan credential tidak disertakan dalam repository.
Tidak ada perubahan billing atau pembelian provider.

Solusi lokal saat ini: observasi paper snapshot berkala melalui RPC yang sudah
aktif, dengan perintah PowerShell berikut setelah build examples:

```powershell
cargo build --release --locked --features yellowstone --bin paper_yellowstone --examples
. .\scripts\Import-PaperProvider.ps1
.\target\release\examples\paper_bootstrap.exe config\paper.local.json --samples 60 --interval-secs 5
```

Default tetap satu snapshot; `--samples` dan `--interval-secs` harus integer
positif. Setiap snapshot memakai pipeline baru, satu context RPC, maksimum
100 explicit accounts, dan tidak mendukung scoped DLMM discovery. Slot sama
atau lebih tua dilewati. Mode ini tidak memalsukan bank identity atau memakai
write version RPC sebagai data Yellowstone. Prediksi opportunity dan landing
memakai prefix `PAPER_RPC_*`; ini sampling berkala, bukan feed HFT, fork replay,
transaction simulation, atau realized PnL. RPC quota tetap berlaku.
Jika landing dikonfigurasi, semua cycle harus mulai dari WSOL agar raw profit
dan biaya lamport seunit; untuk start mint lain, hapus `landing` sebelum observasi.

Untuk menguji layanan gRPC setelah diaktifkan atau memakai provider lain:

```powershell
.\target\release\examples\yellowstone_probe.exe
```

Probe Rust memakai slot-only subscription dan memerlukan processed `bank_id`;
connection, initial subscription, dan penantian slot dibatasi masing-masing
10 detik. Hasil sukses belum membuktikan account ordering/recovery runtime.
Feed produksi kini membatasi connection dan initial subscription masing-masing
15 detik, mempertahankan error feed saat readiness gagal, dan memberi diagnosis
status tanpa mencetak server messages/credential URLs. Bootstrap RPC runtime
juga menghilangkan detail provider yang dapat berisi credential.

Uji binary utama menemukan bug TLS terpisah: `GeyserGrpcClient::build_from_shared`
memakai `Endpoint::from_shared`, yang tidak otomatis mengaktifkan TLS pada
dependency ini. Probe yang mengaktifkan TLS eksplisit mencapai layanan, tetapi
runtime awal berhenti pada `Connect(transport/TLS failure)`. Adapter diperbaiki
dengan `ClientTlsConfig::new().with_native_roots()` untuk URI HTTPS, mengikuti
[contoh resmi Alchemy](https://www.alchemy.com/docs/reference/yellowstone-grpc-quickstart).
Certificate validation tetap aktif. Regression test lokal memeriksa ClientHello
pada wire tanpa memasang certificate atau menonaktifkan validasi.

Verifikasi akhir native Windows:

- `cargo fmt --all -- --check`: lulus.
- `cargo test --locked --all-targets --all-features`: 111 passed, 0 failed,
  28.88 detik, tanpa warning; termasuk TLS handshake, redacted provider errors,
  serta penolakan snapshot parsial/mixed/slot lama.
- `cargo build --release --locked --features yellowstone --bin paper_yellowstone
  --examples`: lulus, 18.55 detik, optimized, tanpa warning.
- Tiga observasi RPC nyata berjarak 5 detik: exit 0, total 10.62 detik,
  context slots `455516482`, `455516501`, `455516525`. Kesembilan account dan
  kedua pool siap pada tiap snapshot. Quote cycle 0: input 100000, output 99409,
  effective -591; cycle 1: input 100000, output 99890, effective -110.
  Tidak ada opportunity yang lolos dan tidak ada transaksi dikirim.
- Binary Yellowstone sesudah perbaikan TLS mencapai subscription dan berhenti
  dengan `Subscribe(gRPC Unimplemented)`, exit 1, 1.97 detik; probe Rust juga
  memperoleh `gRPC Unimplemented`, exit 1, 1.83 detik. Ini kegagalan provider
  yang belum terselesaikan oleh kode, bukan keberhasilan live stream.
- Mock provider lokal: tiga startup failures mempertahankan status asli;
  probe menolak processed tanpa bank id dan menerima marker dengan bank id;
  subscription yang tidak menjawab berhenti setelah 15.03 detik; config
  landing dengan start USDC ditolak sebelum RPC. Exit 1 pada kasus penolakan
  memang diharapkan. Mock success tidak membuktikan akses provider nyata.

Log tersanitasi disimpan di `target/local-setup/tests-tls-final.log`,
`build-tls-final.log`, `rpc-observations.log`, `yellowstone-runtime-tls-fixed.log`,
`yellowstone-probe-final.log`, dan `provider-local-smoke.json`. Instruksi
`--samples 60` tersedia untuk reproduksi; yang diuji ke provider adalah tiga
samples. Semua nilai landing tetap parameter contoh, bukan hasil kalibrasi.

Kebutuhan yang tersisa:

1. Endpoint **Yellowstone gRPC** dengan processed account subscriptions dan
   slot updates, serta optional `x-token`. URL JSON-RPC/WebSocket biasa tidak
   menggantikan endpoint gRPC. Pastikan kompatibilitas provider dengan client
   dan metadata bank/reconnect yang dipakai runtime.
2. RPC Solana pada cluster yang sama: `getMultipleAccounts` sudah diuji.
   Uji `getProgramAccounts` dengan filter pool yang valid jika memakai DLMM.
3. Dua pool nyata, sembilan account dependency, urutan mint, decimals, serta
   raw size limits untuk sampel WSOL/USDC sudah diperiksa dan diisi pada
   config lokal. Pair lain tetap perlu verifikasi yang sama.
4. Bila memakai DLMM: LbPair, kedua mint, initial BinArrays, optional bitmap,
   dan owner/memcmp account filters yang diizinkan provider.

Akses Jito/Helius Sender, saldo trading, wallet, dan private key **tidak
diperlukan** untuk paper mode. Nama provider landing hanya kategori model.
Jangan kalibrasi dari opportunity tanpa label outcome.

## 6. Environment tanpa mencetak token

Kredensial Alchemy sudah disimpan sebagai `SecureString` dalam CLIXML di
`%LOCALAPPDATA%\HFT-Solana\paper-provider.clixml`, di luar working tree.
Windows memakai DPAPI untuk serialisasi nilai rahasia ini; file hanya dapat
didekripsi oleh user Windows yang sama pada komputer yang sama.
[Dokumentasi Microsoft](https://learn.microsoft.com/en-us/powershell/module/Microsoft.PowerShell.Utility/export-clixml?view=powershell-5.1).
Endpoint gRPC dalam file adalah kandidat yang **belum terverifikasi**, dan
`YellowstoneVerified=false`. Untuk memuat environment di PowerShell:

```powershell
Set-Location 'C:\Adn\Adn Belajar python\HFT-Solana'
. .\scripts\Import-PaperProvider.ps1
```

Script telah dijalankan pada Windows PowerShell 5.1 dan dipakai untuk probe
RPC/gRPC di atas. Script memuat empat variabel `HFT_*` tanpa mencetak nilainya;
dekripsi environment berlangsung hanya dalam proses terminal saat ini.
Memuat variabel tidak memulai runtime. File CLIXML tidak dapat dipindahkan ke
PC/user lain untuk dipakai ulang. Jangan tambahkan credential file ke Git.

Untuk mereproduksi pembacaan account di terminal yang sama:

```powershell
$body = '{"id":1,"jsonrpc":"2.0","method":"getAccountInfo","params":["GwsPP9HHhCvEQeu3HTFzsVL6DEtnnYw4ALEtA3fMBC9Q",{"encoding":"base64","commitment":"processed"}]}'
try {
    $reply = Invoke-RestMethod -Uri $env:HFT_SOLANA_RPC_URL -Method Post -ContentType 'application/json' -Body $body -TimeoutSec 15 -ErrorAction Stop
    if ($null -ne $reply.error) {
        Write-Host ('RPC error code: ' + $reply.error.code)
    } else {
        [pscustomobject]@{
            Slot = $reply.result.context.slot
            Exists = ($null -ne $reply.result.value)
            Owner = $reply.result.value.owner
            DataLength = $reply.result.value.space
        }
    }
} catch {
    Write-Host 'RPC request failed. Keep detailed errors local because they may contain credentials.'
}
```

Untuk sesi lokal, gunakan prompt tersembunyi; nilai input tidak menjadi command
literal dalam history. RPC URL dapat mengandung API key, jadi diperlakukan
sebagai rahasia juga.

```powershell
function Set-HftSecretEnvironment {
    param([string]$Name)
    $secret = Read-Host $Name -AsSecureString
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secret)
    try {
        [Environment]::SetEnvironmentVariable(
            $Name,
            [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer),
            [EnvironmentVariableTarget]::Process
        )
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
        $secret.Dispose()
    }
}
Set-HftSecretEnvironment 'HFT_YELLOWSTONE_ENDPOINT'
# Jalankan baris berikut hanya jika provider membutuhkan x-token.
Set-HftSecretEnvironment 'HFT_YELLOWSTONE_X_TOKEN'
Set-HftSecretEnvironment 'HFT_SOLANA_RPC_URL'
$env:HFT_BOOTSTRAP_TIMEOUT_SECS = '30'
```

Nilai environment proses tetap berupa plaintext di memori karena diperlukan
client; jangan dump environment atau menyalin log error mentah ke publik.
Untuk penyimpanan permanen gunakan Windows Credential Manager/secret vault
yang disetujui, lalu muat ke environment proses; tidak ada file token plaintext
atau environment permanen yang dibuat dalam setup ini. Runtime tidak memuat
`.env` secara otomatis. Tutup terminal atau bersihkan setelah selesai:

```powershell
'HFT_YELLOWSTONE_ENDPOINT','HFT_YELLOWSTONE_X_TOKEN','HFT_SOLANA_RPC_URL' |
    ForEach-Object { Remove-Item -LiteralPath ('Env:' + $_) -ErrorAction SilentlyContinue }
```

## 7. Jalankan dan amati setelah config/provider siap

```powershell
cargo run --release --locked --features yellowstone --bin paper_yellowstone -- config/paper.local.json
# Atau langsung jalankan artifact yang sudah dibangun:
.\target\release\paper_yellowstone.exe config/paper.local.json
```

Periksa startup subscription, kemudian bootstrap RPC dan buffered catch-up.
Format log berikut diambil dari source; **belum merupakan hasil pengamatan
provider nyata**:

```text
paper mode: subscribing to <count> explicit accounts and <count> DLMM bin scopes; live transaction submission is disabled
paper bootstrap: explicit_accounts=<count> dlmm_bin_arrays=<count> context_slot=<slot> rpc_elapsed_ms=<ms> ... buffered_applied=<count> buffered_stale=<count> initial_opportunities=<count>
PAPER_OPPORTUNITY cycle=<id> amount_in=<raw> expected_out=<raw> effective_profit=<raw> ... output_queue_age_ns=<ns>
PAPER_LANDING cycle=<id> provider=<model> ... net_if_landed=<raw> expected_value=<raw>
PAPER_LANDING_SKIP cycle=<id> effective_profit=<raw>
PAPER_DLMM_REFRESH_REQUIRED pool=<id> missing_accounts=<public account keys>
```

Tidak munculnya opportunity bukan bukti feed gagal atau profit pasti tersedia.
DLMM yang window-nya belum lengkap tetap inactive. Reconnect/fork rollback dan
slot fences perlu diamati dengan provider nyata selain deterministic tests.

## Known limitations dan troubleshooting

- Belum ada offline replay CLI untuk `paper_yellowstone`. Offline coverage
  tersedia melalui integration tests/fixtures. `calibrate_landing` khusus
  labeled outcome JSONL; tidak dijalankan tanpa data berlabel.
- Aggregate `paper stopped:` berisi invalidations, dirty coalescing, drops,
  processing duration, dan landing EV. Source belum memasang graceful Ctrl+C
  handler; penghentian paksa dapat melewatkan aggregate tersebut.
- `cargo` tidak ditemukan: tambahkan `.cargo\bin` ke PATH sesi seperti di atas.
- `link.exe` tidak ditemukan / `LNK1104`: pastikan MSVC x64/x86 dan Windows SDK
  terpasang lengkap; gunakan Developer PowerShell bila auto-discovery gagal.
- Error C/C++/protobuf/libclang: baca crate/build-script yang benar-benar gagal
  dahulu. Jangan memasang paket tambahan atau berpindah OS tanpa bukti.
- Disk penuh: hentikan build, cek ruang, atau pilih disk lain untuk
  `CARGO_TARGET_DIR`. Jangan menghapus file pengguna secara otomatis.
- Dependency fetch gagal: cek crates.io dan dua Git revisions di `Cargo.toml`;
  jangan mengganti pin SDK hanya agar build lewat.
- `InvalidPubkey` pada `REPLACE_WITH_*`: lengkapi config, bukan masalah provider.
- Timeout bootstrap: pastikan RPC dan cluster sesuai, rate limit memadai;
  timeout hanya membatasi bootstrap RPC, bukan seluruh startup gRPC.
- `gRPC Unimplemented`: periksa endpoint Yellowstone dan aktivasi streaming.
  Pada akun Alchemy yang diperiksa, pemilik mengonfirmasi kebutuhan upgrade.
  Gunakan `paper_bootstrap --samples N --interval-secs 5` untuk observasi RPC
  sementara; pergantian paket/provider masih memerlukan verifikasi metadata bank.
- Slot fence lama menunggu `SLOT_COMPLETED` dengan `bank_id`, tetapi status itu
  merujuk shred reception dan tidak terkait bank pada schema dependency.
  Adapter telah diperbaiki memakai `SLOT_PROCESSED` dengan bank identity asli;
  [dokumentasi Agave](https://docs.anza.xyz/validator/geyser) menyatakan account
  notifications mendahului processed. Regression DLMM melalui protobuf adapter
  gagal sebelum perubahan (exit 101) dan lulus sesudahnya. Marker tanpa
  `bank_id` tetap ditolak. Urutan end-to-end provider dan reconnect tetap
  memerlukan pengamatan live; keberhasilan tests tidak membuktikan kompatibilitas
  setiap provider Yellowstone.
- Simpan credential lokal; sanitasi URL query/path, Authorization headers, dan
  x-token sebelum membagikan error/log.

## Review dan publish perubahan setup

PR #1 sudah merged. Perubahan setup/provider dikerjakan pada branch terpisah
`dev/local-paper-provider`; semua file config lokal dan log di-ignore Git.
Untuk mereproduksi publikasi setelah review:

```powershell
git switch dev/local-paper-provider
git add .gitignore README.md Cargo.toml Cargo.lock docs/LOCAL_SETUP.md docs/PAPER_RUN.md scripts/Import-PaperProvider.ps1 vendor/yellowstone-grpc-client examples src/feed/types.rs src/feed/yellowstone.rs src/bin/paper_yellowstone.rs tests/meteora_dlmm_reactor.rs tests/yellowstone_request.rs
git diff --cached --stat
git diff --cached --check
git commit -m 'fix: support Windows paper setup and provider diagnostics'
git push -u origin dev/local-paper-provider
gh pr create --draft --base main --head dev/local-paper-provider --title 'Support Windows paper setup and RPC snapshot observations'
```

File config lokal dan log tidak termasuk daftar staging. Status publikasi aktual
dan CI dilaporkan setelah commit/push diverifikasi. Tidak ada commit atau push
langsung ke `main` yang diperlukan untuk menjalankan lokal.

## Pengamatan pool yang diperluas — 11 Oktober 2026

Discovery dari indeks Raydium/Meteora dan petunjuk DEX Screener menghasilkan
29 kandidat pada delapan pasangan. Label indeks tidak dipakai sebagai bukti
program pool: validator Rust menolak owner/layout yang tidak didukung, membaca
mint dan vault dari state on-chain, memeriksa activation/status, kemudian
memastikan seluruh dependency dan quote snapshots siap.

Config baru `config/paper.expanded.local.json` berhasil dibuat tanpa mengubah
config dua pool sebelumnya. Hasil validasi pada context slot `455545886`:

- 10 pool: 1 PumpSwap, 3 Raydium CPMM, dan 6 Meteora DAMM v2.
- 5 mint legacy SPL initialized, 4 pasangan WSOL, 16 cycle dua-leg berarah.
- 41 dependency accounts, masih dalam satu `getMultipleAccounts` response.
- Decimals mint diverifikasi ulang pada slot `455548421`: WSOL 9, mint
  `6NwarBvDkXhByqVp2Qkq5i9XbtA2B3Bwe8SWGu9vpump` 6,
  `A3vBwL3Pd6nkBqVo87D6TLWYG4ekWuDeqCoLFeyvRMcb` 5,
  `8Ne5fhU1B7RuBB6v3GhJ9ixuLUJYcRfP9MCEhCuNtsss` 6, dan
  `HgBRWfYxEfvPhtqkaeymCQtHCrKE46qQ43pKe8HCpump` 6.

Settings search dan landing tetap diwarisi dari config lokal sebelumnya:
probe 100.000, seed 1.000.000, max size 10.000.000, minimum effective profit
1.000 lamport. Seluruh cycle kembali ke WSOL; nominal token lain tidak dijumlahkan
langsung ke biaya SOL.

Perintah observasi yang benar-benar dijalankan:

```powershell
. .\scripts\Import-PaperProvider.ps1
.\target\release\examples\paper_bootstrap.exe config\paper.expanded.local.json --samples 120 --interval-secs 5 --diagnostic-sizes 100000,1000000,10000000 --report target\local-setup\paper-expanded-20261011-130611.json
```

Exit 0, durasi proses 613,42 detik. Context slot naik dari `455546047` ke
`455548858`. Semua 120 snapshot diterima; tidak ada snapshot skip, quote error,
atau pool yang gagal quote-ready. Search mengevaluasi 1.920 state vector cycle;
seluruhnya berhenti pada probe nonpositif. Counter duplicate 1.920 adalah
dedupe saat cycle yang sama dikunjungi melalui kedua pool di dalam satu
pipeline; bukan kegagalan penerimaan snapshot RPC.

Quote probe dan dua ukuran diagnosis menghasilkan 5.760 pembacaan, seluruhnya
negatif. Tidak ada kasus probe negatif tetapi grid ukuran lebih besar positif.
Hasil terbaik dari seluruh cycle/waktu untuk masing-masing input:

| Raw input WSOL | SOL | Effective profit terbaik sebelum biaya eksekusi |
| ---: | ---: | ---: |
| 100000 | 0,0001 | -632 lamport |
| 1000000 | 0,001 | -6323 lamport |
| 10000000 | 0,01 | -65421 lamport |

Opportunity lolos: **0**; landing selected/rejected: **0/0** karena belum ada
opportunity yang masuk policy. Tidak ada transaksi dikirim. Dua belas sampel
awal (~57 detik) juga menghasilkan nol opportunity.

Dari 119 interval setelah baseline, 106 memiliki account payload yang sama
dan 13 berubah. Slot yang bertambah tidak berarti pool mengalami swap.
Pengamatan ini mendukung prioritas berikutnya: ukur aktivitas pool yang nyata
dan pilih pasangan dengan beberapa pool yang aktif serta fee yang dapat
ditutup spread. Menambah jumlah pool saja belum menghasilkan edge positif.
Grid terbatas tidak membuktikan ketiadaan semua peluang di ukuran/waktu lain.

Log dan report tersanitasi disimpan lokal di
`target/local-setup/paper-expanded-20261011-130611.log` dan `.json`.
Langkah discovery lintas OS serta arti seluruh counter tersedia di
[PAPER_DISCOVERY.md](PAPER_DISCOVERY.md). Filter profit tidak dilonggarkan.
Yellowstone, recovery/reconnect provider, dan live slot fences tetap belum
terverifikasi karena akses gRPC belum tersedia. Tidak ada upgrade paket.

Verifikasi perubahan ini: `cargo fmt --all -- --check` lulus;
`cargo test --locked --all-targets --all-features` lulus **118 test, 0 gagal,
tanpa warning** pada native Windows; tiga unittest discovery Python juga lulus.
Test profile default selesai kompilasi dalam 1 menit 32 detik setelah resource
PC membaik.

Percobaan awal compiler sempat gagal alokasi memori ketika drive C: hanya
tersisa ~200 MB dan Windows commit memory hampir penuh. Artifact yang
terganggu menghasilkan error E0463/E0462 pada percobaan berikutnya, termasuk
percobaan release dan codegen-unit khusus. Setelah pengguna menutup aplikasi
berat, tersedia ~11 GB RAM dan ~4,8 GB disk. Package-scoped
`cargo clean -p hft-solana --profile dev` dan pengulangan command test normal
dengan `CARGO_BUILD_JOBS=1` berhasil. Tidak diperlukan perubahan math/dependency
untuk menyelesaikan kegagalan resource tersebut. Pembersihan direktori penuh
melalui shell ditolak pemeriksaan otomatis; Cargo hanya membersihkan artifact
paket aplikasi. Config, credential, dependency builds, dan source tetap tersimpan.

## Pemilihan berdasarkan aktivitas account - 11 Oktober 2026

Discovery berikutnya menghasilkan 30 kandidat pada delapan pasangan mint.
Sepuluh pool native yang terhubung memenuhi pemeriksaan owner/layout/mint dan
activation awal. Mode baru mengukur account pool dan kedua vault, bukan hanya
volume historis dari indeks. Slot yang meningkat dengan payload identik tidak
menaikkan peringkat; duplicate/old contexts tidak mengganti baseline.

Satu pengukuran 13 sampel tidak menemukan perubahan dan berhenti tanpa config.
Pengukuran berikutnya diperpanjang menjadi 37 sampel dengan jeda lima detik:
188,02 detik, 37 request RPC, context `455570890`..`455571761`. Dua pool
Raydium CPMM masing-masing berubah pada dua dari 36 interval yang dibandingkan;
delapan pool lain tidak berubah. Config terpilih divalidasi kembali pada satu
context `455571761`: dua pool, dua mint, dua cycle berarah, 10 dependency accounts.

Pool terpilih adalah `H5gndJaYQn59VaNd7iPnW4KS1kisk8sG4UMeZAtjX4WZ` dan
`73frby6kZ8NwxmmBgdJ8sXmgsXbi3rAv6vuDAXh1We9x`, pada pasangan WSOL dan
`A3vBwL3Pd6nkBqVo87D6TLWYG4ekWuDeqCoLFeyvRMcb`. Semua alamat berasal dari
state yang diverifikasi. Config lama tetap tersedia. Search/landing settings,
raw-unit bounds, dan minimum effective profit tidak berubah.

Perintah yang dijalankan setelah memuat provider lokal:

```powershell
. .\scripts\Import-PaperProvider.ps1
.\target\release\examples\paper_discover.exe target\local-setup\paper-active-candidates-20261011-142619.json config\paper.local.json config\paper.active-observed-20261011-143618.local.json 15 --activity-samples 37 --activity-interval-secs 5 --activity-report target\local-setup\paper-activity-observed-20261011-143618.json
.\target\release\examples\paper_bootstrap.exe config\paper.active-observed-20261011-143618.local.json --samples 120 --interval-secs 5 --diagnostic-sizes 100000,1000000,10000000 --report target\local-setup\paper-active-observed-20261011-143618.json
```

Pemeriksaan tambahan atas 30 account pool/vault menghasilkan payload identik
dari RPC terkonfigurasi (context `455571733`) dan RPC publik mainnet-beta
(`455571735`). Ini adalah pemeriksaan satu waktu pada context independen,
bukan bukti bahwa provider tidak pernah terlambat atau berbeda fork.

Boundary tanpa aktivitas juga diperiksa memakai server RPC lokal di loopback:
payload publik tetap, context slot meningkat, tiga sampel. Program mencatat
dua interval per pool, nol perubahan, menulis report
`status=no_qualified_active_pair`, exit 1, dan tidak membuat config. Empat
request server seluruhnya `getMultipleAccounts`. Regresi unit mencakup vault
changes, duplicate/old contexts, partial/mixed contexts, ordering akun, ranking
fraction, quiet-pair exclusion, static counterpart, dan CLI request bounds.

Verifikasi source: format lulus, **126 tes Rust dan tiga tes Python lulus**,
release Yellowstone binary dan examples lulus tanpa warning. Build release
terakhir tercatat 10,98 detik dengan satu Cargo job. Ini bukan benchmark hot path.
Pemindaian 126 file Git index tidak menemukan token/RPC URL provider. Config,
laporan aktivitas, log, fixture pemeriksaan lokal, dan kredensial tetap di luar Git.

Pengamatan paper config terpilih selesai dengan exit 0 dalam 613,72 detik.
Semua 120 snapshot diterima, dari context `455571763` sampai `455574577`.
240 cycle state vectors dievaluasi, semuanya ditolak pada probe nonpositif;
240 duplicate adalah dedupe dalam pipeline. Probe dan dua ukuran tambahan
menghasilkan 720 quote, seluruhnya negatif. Quote failures, skipped samples,
probe/grid disagreements, opportunities, dan transaksi terkirim semuanya nol.
Landing selected/rejected 0/0 karena tidak ada opportunity yang masuk policy.

| Raw input WSOL | SOL | Effective profit terbaik sebelum biaya eksekusi |
| ---: | ---: | ---: |
| 100000 | 0,0001 | -170 lamport |
| 1000000 | 0,001 | -1768 lamport |
| 10000000 | 0,01 | -24781 lamport |

Dari 119 interval setelah baseline, 10 berubah dan 109 tidak berubah.
Run sebelumnya dengan 10 pool memiliki 13 interval perubahan. Pemilihan dari
aktivitas teramati tidak membuktikan aktivitas akan berlanjut: sampling pendek
bisa melewatkan burst, dan pasar/semesta pool berbeda antara kedua run.
Untuk dua cycle yang sama, profit terbaik pada input 100.000 sebelumnya
adalah -1.001 lamport, kini -170 lamport. Quote lebih dekat ke titik impas,
tetapi masih belum positif.
Ini bukan klaim peningkatan performa terkontrol atau realized PnL.

Report akhir: `target/local-setup/paper-active-observed-20261011-143618.json`;
log dengan basename yang sama berakhiran `.log`. Report aktivitas:
`target/local-setup/paper-activity-observed-20261011-143618.json`.
Untuk menjalankan ulang, gunakan path baru pada panduan
[PAPER_DISCOVERY.md](PAPER_DISCOVERY.md); output lama tidak ditimpa.
Yellowstone tetap belum diuji dan membutuhkan akses provider yang kompatibel.
