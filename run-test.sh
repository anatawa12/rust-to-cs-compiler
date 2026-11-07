#!/bin/sh

set -eu

cargo build --release

time RUST_BACKTRACE=1 ./target/release/rust-to-cs-compiler --manifest-path=exstd/exalloc --crate-name=exalloc > dotnet/r2CsRuntime/Alloc/Generated.cs.tmp \
  && cat < dotnet/r2CsRuntime/Alloc/Generated.cs.tmp > dotnet/r2CsRuntime/Alloc/Generated.cs
rm dotnet/r2CsRuntime/Alloc/Generated.cs.tmp

time RUST_BACKTRACE=1 ./target/release/rust-to-cs-compiler \
  --manifest-path="./vrc-get/vrc-get-vpm/Cargo.toml" \
  --crate-name="vrc_get_vpm" \
  > dotnet/r2CsNative/Out.cs.tmp \
  && <dotnet/r2CsNative/Out.cs.tmp sed \
     -e 's#byte\[\].m_FromHex#byteArray.m_FromHex#' \
    > dotnet/r2CsNative/Out.cs \
&& git diff --no-index -R dotnet/r2CsNative/Out.cs dotnet/r2CsNative/Out.bak.cs -U10
