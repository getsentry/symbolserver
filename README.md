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
  healthcheck_interval: 60
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
* `SYMBOLSERVER_HEALTHCHECK_INTERVAL` (used if `server.healthcheck_interval` is not set)
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

## For Local Development

If you are doing local development with in the getsentry org and you want to use the
symbol server you can follow this guide.  Before you do please make sure to install
a reasonable recent version of Rust with rustup.

First create a `symbolserver.yml`:

```yaml
aws:
  # Ask matt for these
  access_key: MY_ACCESS_KEY
  secret_key: MY_SECRET_KEY
  bucket_url: BUCKET_URL

# Server directory
symbol_dir: /Users/your-username/.sentry-symbols

# Only sync the latest SDKs.  Chances are this is all you have locally anyways
sync:
  interval: 500
  ignore:
    - '*'
    - '!iOS_10.*'
```

Then make sure to create the folder for the symbols:

```
mkdir /Users/your-username/.sentry-symbols
```

Then run the server like this:

```
cargo run -- --config=symbolserver.yml run
```

The server will spin up, start synching symbols and eventually you are ready to go.

## SDK Processing

If you are tasked with process SDK files this is how you do it:

1.  connect an i-device (like an iPod) running the version of the
    operating system you want to extract the symbols of.
2.  launch Xcode and the device manager there.  It might be necesssary to
    "use this device or development".
3.  wait for Xcode to finish processing the device.
4.  go to `~/Library/Developer/Xcode/iOS DeviceSupport` (or tvOS etc.)
5.  ensure a folder there was created for the version of iOS you are
    running.
6.  zip the entire thing up, then store it in the S3 bucket for original
    SDKs
7.  next let the symbol server process the file:

    ::

        sentry-symbolserver -- convert-sdk --compress "~/Library/Developers/Xcode/iOS DeviceSupport/X.Y.Z (WWWWW)

8.  the generated file is dumped into the current working directory and you
    can then upload it to the S3 bucket where memdb files go.
