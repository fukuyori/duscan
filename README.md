# duscan

`duscan` summarizes disk usage by directory depth.

It scans a target path, groups entries at the requested depth, and prints:

- total size
- file count
- oldest file modification time
- newest file modification time

Times are shown in the local timezone.

## Usage

```sh
duscan [--depth N] [--sort size|files|newest|oldest [asc|desc]] [PATH]
```

If `PATH` is omitted, the current directory is scanned.

Examples:

```sh
duscan .
duscan --depth 2 .
duscan --sort size desc .
duscan --sort oldest desc .
duscan --depth 3 --sort newest asc /some/path
```

## Options

- `-d, --depth N`: group results at depth `N`; default is `1`
- `-s, --sort KEY [ORDER]`: sort by `size`, `files`, `newest`, or `oldest`
- `ORDER`: `asc` or `desc`

Default sort orders:

- `size`: `desc`
- `files`: `desc`
- `newest`: `desc`
- `oldest`: `asc`

## Build

```sh
cargo build --release
```

## Releases

Push a version tag such as `v0.4.0` to build ZIP archives and publish a GitHub Release.
Archives are provided for Linux x86_64, Windows x86_64, macOS Apple silicon, and macOS Intel.
Each archive contains the executable, this README, and the Apache 2.0 license.

## License

Licensed under the Apache License, Version 2.0.
