#!/bin/bash
set -eu

aliases='1 1.4 latest'
tag='getsentry/symbolserver'

fullVersion=$(awk '$1 == "ENV" && $2 == "SYMBOLSERVER_VERSION" { print $3; exit }' Dockerfile)

docker build --pull --rm -t $tag:$fullVersion .
docker push $tag:$fullVersion

for alias in $aliases; do
    docker tag $tag:$fullVersion $tag:$alias
    docker push $tag:$alias
done
