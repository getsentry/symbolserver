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
        set -- gosu symbolserver "$@"
    fi
fi

exec "$@"
