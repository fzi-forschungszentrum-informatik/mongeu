# mongeu -- MONitor for GPU energy Usage

This program provides a RESTful API via HTTP for measuring the energy
consumption of GPUs and retrieving additional information. Currently, only
nVidia GPUs are supported (via NVML).

The API is [documented](./openapi.yaml) in the OpenAPI 3.1 format.

## Usage

The following command line options are recognized:
* `-l --listen <ADDR>`: address to listen on for connections. Currently, the
  address must be specified as a numeric IPv4- or IPv4-address. Multiple
  addresses may be specified. By default, the service accepts connections via
  all IPv4 and IPv6 addresses.
* `-p --port <PORT>`: port to listen on for connections. By default, the service
  listens on port 80.
* `--base-uri <URI>`: specifies an URI under which the API is served. The base
  URI will be used when constructing redirects. This is only relevant if the
  service is placed behind a reverse proxy.
* `--enable-oneshot`: enable one-shot (single-request) measurements.
* `--oneshot-duration <MILLISECS>`: specifies a default duration for one-shot
  measurements in `ms`.
* `--gc-min-age <SECONDS>`: age at which campaigns might be collected in `s`.
  Any campaign older than this value is subject to garbage collection. Defaults
  to `86400` (one day).
* `--gc-min-campaigns <NUM>`: number of campaigns required for garbage
  collection. This value must be greater than `0`. Garbage collection is only
  performed when more than this number of campaigns are currently active.
  Defaults to `65536`.
* `-c --config <FILE>`: read configuration from the provided configuration file.
* `-v --verbose`: increase verbosity level. May be specified multiple times.
* `-h --help`: display help
* `-V --version`: display version

Note that, currently, deamonization is not supported.

If no config file is provided via the `-c`/`--config` command line option,
`mongeu` will try to retrieve configuration from `/etc/mongeu.toml` if such a
file exists.

## Config file format

The configuration may be supplied via the `-c` option in the form of a TOML
file. The following items are recognized:

* `[network]`: (optional) section defining the network setup, containing:
  * `[[network.listen]]`: entries defining IP/port combinations to listen on. If
    no entries are supplied, the service will accept connections via all IPv4
    and IPv6 addresses. An entry contains:
    * `ip`: mandatory IP as a string
    * `port`: (optional) port to listen on for connections. If not provided, the
      default port will be used.
  * `port`: (optional) default port to listen on if no port is defined for a
    given entry. Defaults to `80`.
* `[oneshot]`: (optional) section defining configuration of one-shot
  (single-request) endpoints, containing:
  * `enable`: (optional) enable one-shot measurements. Defaults to `false`.
  * `duration`: (optional) default duration for one-shot measurements (in `ms`).
    Defaults to `500`.
* `[gc]`: (optional) section configuring garbage collection, containing:
  * `min_age`: (optional) age at which campaigns might be collected in `s`.
    Defaults to `86400` (one day).
  * `min_campaigns`: (optional) number of campaigns required for garbage
    collection. Defaults to `65536`.
* `base_uri`: (optional) URI under which the API is served.

Command line options override values from config files.

## Building

This project is built using Cargo, the Rust package manager.
