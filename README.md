# GPU measurement service

This program provides a RESTful API via HTTP for measuring the energy
consumption of GPUs and retrieving additional information. Currently, only
nVidia GPUs are supported (via NVML).

The API is [documented](./openapi.yaml) in the OpenAPI 3.1 format.

## Usage

The following command line options are recognized:
* `-l --listen <ADDR>`: address to listen on for connections. Currently, the
  address must be specified as a numeric IPv4- or IPv4-address. By default,
  the service listens on all IPv6 addresses.
* `-p --port <PORT>`: port to listen on for connections. By default, the service
  listens on port 80.
* `--base-uri <URI>`: specifies an URI under which the API is served. The base
  URI will be used when constructing redirects. This is only relevant if the
  service is placed behind a reverse proxy.
* `--oneshot-duration <MILLISECS>`: specifies a default duration for one-shot
  measurements in `ms`.
* `--gc-min-age <SECONDS>`: age at which campaigns might be collected in `s`.
  Any campaign older than this value is subject to garbage collection. Defaults
  to `86400` (one day).
* `--gc-min-campaigns <NUM>`: number of campaigns required for garbage
  collection. This value must be greater than `0`. Garbage collection is only
  performed when more than this number of campaigns are currently active.
  Defaults to `65536`.
* `-v --verbose`: increase verbosity level. May be specified multiple times.
* `-h --help`: display help
* `-V --version`: display version

Note that, currently, deamonization is not supported.

## Building

This project is built using Cargo, the Rust package manager.