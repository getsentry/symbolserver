# Sentry Symbol Server

[![Build Status](https://travis-ci.org/getsentry/symbolserver.svg?branch=master)](https://travis-ci.org/getsentry/symbolserver)

This repository implements a symbol server service which Sentry uses for
symbolication of frames from Apple system SDKs.  If you have Apple SDKs
you can use the `convert-sdk` command to convert them into a memdb file
and store it on S3.  The server will then synchronize with that bucket
and serve up symbols on an API.

## Configuration

The server by default looks for a YAML config file in
`~/.sentry-symbolserver.yml` and will load that.  Additionally various
configuration options can be supplied on the command line or in some
cases as environment variables.

Example config with all keys:

```yaml
# The AWS credentials for synchronization
aws:
  access_key: MY_ACCESS_KEY
  secret_key: MY_SECRET_KEY
  bucket_url: s3://BUCKET/PATH
  region: AWS_REGION_NAME

# Server directory
symbol_dir: /path/to/symbol/directory

# Where we listen for http
server:
  host: '127.0.0.1'
  port: 3000
  # Cache the healthcheck for 60 seconds
  healthcheck_ttl: 60
  # Sync every 2 minutes

# Controls the sync
sync:
  interval: 120
  ignore:
    - '*'
    - '!iOS_10.*'
    - '!iOS_9.*'

# Log stuff
log:
  # Log leve (trace, debug, info, warning, error)
  level: trace
  # If this is set to null the server will log to stderr
  file: /path/to/logfile.log
```

## Environment Variables

The following environment variables are picked up in accordance with
AWS conventions.

* `AWS_ACCESS_KEY_ID` (overrides `aws.access_key`)
* `AWS_SECRET_ACCESS_KEY` (overrides `aws.secret_key`)
* `AWS_SESSION_TOKEN` (no config equivalent)
* `AWS_DEFAULT_REGION` (used if `aws.region` is not set)

Symbolserver specific variables:

* `SYMBOLSERVER_BUCKET_URL` (used if `aws.bucket_url` is not set)
* `SYMBOLSERVER_SYMBOL_DIR` (used if `symbol_dir` is not set)
* `SYMBOLSERVER_LOG_LEVEL` (used if `log.level` is not set)
* `SYMBOLSERVER_LOG_FILE` (used if `log.file` is not set)
* `SYMBOLSERVER_HEALTHCHECK_TTL` (used if `server.healthcheck_ttl` is not set)
* `SYMBOLSERVER_SYNC_INTERVAL` (used if `server.sync_interval` is not set)
* `SYMBOLSERVER_THREADS` (used as a default for `run --threads`)

Additionally these well known variables are supported:

* `IP` and `PORT` (used as a default for `run --bind`)
* `LISTEN_FDS` (used as default for `run --bind-fd` in systemd style. eg: listens on FD 3)
* `http_proxy` (no config equivalent)

## Endpoints

The following API endpoints exist:

`GET /health`
> A simple healthcheck that reports 200 if everything is okay, or 502 otherwise.  It
> also contains a JSON payload with the sync lag (number of unsynchronized SDKs).

`GET /sdks`
> Returns a list of SDKs that the server is currently serving up

`POST /lookup`
> Performs a symbol lookup.  For request or response format look into the
> [api::handlers](https://github.com/getsentry/symbolserver/blob/master/src/api/handlers.rs)
> module.
