#!/bin/sh

prepare_patched() {
  crate=$1
  version=$2
  shift
  shift

  ap_name="ra_ap_$crate"

  rm -rf "patched/$ap_name-$version"
  mkdir -p "patched/$ap_name-$version" || exit 1
  curl -sL "https://static.crates.io/crates/$ap_name/$ap_name-$version.crate" \
    | tar x -C "patched/$ap_name-$version" --strip-components=1

  while [ $# != 0 ]; do
    echo "applying $1: $2"
    curl -sL "https://github.com/rust-lang/rust-analyzer/commit/$1.patch" \
      | awk "/^diff --git a\// {p=0} /^diff --git a\/crates\/$crate\// {p=1} p" \
      | git apply -v -p3 --directory="patched/$ap_name-$version" --allow-empty -

    shift # skip commit
    shift # skip comment
  done
}

prepare_patched hir 0.0.351 \
  1c4c342b0daecbd5731fabfd053fff460cdaaff7 "PR#23352 panic when we call impls_trait for self type of builtin derive impls for generic types" \
