#!/bin/sh
set -ex
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
cross build --target $TARGET --release
