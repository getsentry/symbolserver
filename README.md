# Sentry Symbol Server

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
  sync_interval: 120

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

* `AWS_ACCESS_KEY_ID`
* `AWS_SECRET_ACCESS_KEY`
* `AWS_SESSION_TOKEN`
* `AWS_DEFAULT_REGION`

Additionally these are supported:

* `SYMBOLSERVER_SYMBOL_DIR`
* `http_proxy`
