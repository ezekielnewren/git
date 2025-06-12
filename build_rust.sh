#!/bin/sh

build_dir=$1
rust_target=$2
crate=$3

if [ "$build_dir" == "" ]; then
  echo "did not specify the build directory"
  exit 1
fi

if [ "$rust_target" == "" ]; then
  echo "did not specify the build directory"
  exit 1
fi

export CARGO_TARGET_DIR=$build_dir/rust_build

if [ "$rust_target" = "release" ]; then
  rust_args="--release"
  export RUSTFLAGS='-Aunused_imports -Adead_code'
elif [ "$rust_target" = "debug" ]; then
  rust_args=""
  export RUSTFLAGS='-Aunused_imports -Adead_code -C debuginfo=2 -C opt-level=1 -C force-frame-pointers=yes'
else
  echo "illegal rust_target value $rust_target"
  exit 1
fi

cargo build --manifest-path $build_dir/../rust/Cargo.toml -p $crate $rust_args

src=$CARGO_TARGET_DIR/$rust_target/lib${crate}.a
dst=$build_dir/lib${crate}.a

rm $dst
mv $src $dst
