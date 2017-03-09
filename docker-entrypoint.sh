#!/bin/sh
set -e

if [ "${1#-}" != "$1" ]; then
    set -- symbolserver "$@"
fi

if symbolserver "$1" -h > /dev/null 2>&1; then
    set -- symbolserver "$@"
fi

if [ "$1" = 'symbolserver' ]; then
    set -- tini -- "$@"
    if [ "$(id -u)" = '0' ]; then
        mkdir -p "$SYMBOL_DIR"
        chown -R symbolserver "$SYMBOL_DIR"
        set -- gosu symbolserver "$@"
    fi
fi

exec "$@"
