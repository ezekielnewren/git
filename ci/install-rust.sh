#!/bin/sh
#
# Install rust
#

begin_group "Install rust"

if [ "$CARGO_HOME" == "" ]; then
  echo >&2 "::error:: CARGO_HOME is not set"
  exit 1
fi

## install rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain none -y
if [ ! -f $CARGO_HOME/env ]; then
  echo "PATH=$CARGO_HOME/bin:\$PATH" > $CARGO_HOME/env
fi
# . $CARGO_HOME/env
## install a specific version of rust
$CARGO_HOME/bin/rustup default $RUST_VERSION || exit 1
## non root user's need write access to $CARGO_HOME to install crates
chmod 1777 $CARGO_HOME

end_group "Install rust"
