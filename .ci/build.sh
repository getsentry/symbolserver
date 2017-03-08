#!/bin/sh
set -ex
cross build --target $TARGET --release
