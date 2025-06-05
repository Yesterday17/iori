# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2] - 2025-06-05

### New Features

- Supported Niconico video download.

## [0.2.1] - 2025-06-05

### New Features

- Supported decryption of Sample-AES Elementary Audio Stream Setup.
- Supported concat merge for `aac` format.
- Added `DashInspector` to match `.mpd` manifests.
- **Experimental**: Supported download for MPEG-DASH live stream.

### Fixed

- Fixed a crash caused by i18n bundle on windows. ([#16](https://github.com/Yesterday17/iori/issues/16))
- CacheSource failure can be retried correctly now.
- Fixed potential segment duplication when using `-m` for in-memory cache.

## [0.2.0] - 2025-05-11

### Breaking Changes

- Changed the environment variable to control `temp_dir` form `TEMP` to `TEMP_DIR`.
- Updated inspector argument input. Now you should use the following arguments directly instead of using `-e/--args`:
  - `nico-user-session`
  - `nico-download-danmaku`
  - `nico-chase-play`
  - `nico-reserve-timeshift`
  - `nico-danmaku-only`
  - `showroom-user-session`

### New Features

- **Nicolive**: Added `--nico-chase-play` to download nico live from start.
- **Nicolive**: Added `--nico-reserve-timeshift` to reserve timeshift automatically.
- **Nicolive**: Added `--nico-danmaku-only` to control whether to skip video download.
- **Gigafile**: Experimental support for downloading file from [Gigafile](https://gigafile.nu/).
- Added `--http1` flag to force the http client to connect with `HTTP 1.1`.

### Updated

- Changed default temp dir to `current_dir` instead of `temp_dir`.
- Added some i18n for command line options.
- **iori**: Segments from different streams will be mixed before download. This makes `--pipe-mux` available to play vods.
- **Nicolive**: Added `frontend_id` to `webSocketUrl` to match the behavior of web.
- **Nicolive**: Supported reconnection for Nicolive `WebSocket` client.
- **Nicolive**: Optimized `xml2ass` logic.
- Supported experimental `opendal` cache source in `iori` with `--opendal` flag.

### Fixed

- Fixed panic on error occurs when using `--wait` in `shiori download`.
- Fixed issue where the `--pipe` argument was not working.
- **Nicolive**: Fixed panic when operator is not found in `xml2ass`.
- **iori**: Fixed an issue with m3u8 retrieval intervals caused by precision problems.

## [0.1.4] - 2025-04-16

### Updated

- **NicoLive**: Supported danmaku download.
- **Showroom**: Supported timeshift download.
- File extension would be appended to the output file automatically.
- Improved help messages for inspectors.

### Fixed

- **NicoLive**: `NicoLiveInspector` now extracts the best quality stream.
- **NicoLive**: `NicoLiveInspector` now always uses `http1` for `WebSocket` connection.


## [0.1.3] - 2025-03-28

### Fixed

- Hotfix for download command.

### Updated

- Increased timeout for update check to 5 seconds.
- Upgraded clap to 4.5.34.

## 0.1.2 - 2025-03-28

### Added

- Added auto update check after download.
- Added `--skip-update` option to skip update check.
- Added `update` subcommand to upgrade shiori to the latest version.

### Fixed

- Downloaded `cmfv` and `cmfa` segments will have correct extension.

## [0.1.1] - 2025-03-28

### Added

- Declare [`longPathAware`](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation?tabs=registry#application-manifest-updates-to-declare-long-path-capability) to support long path on Windows.

### Fixed

- Merge failure with `mkvmerge` when there are too many segments.

## [0.1.0] - 2025-03-27

### Added

- `Nico Timeshift` support.

[0.1.0]: https://github.com/Yesterday17/iori/tree/shiori-v0.1.0
[0.1.1]: https://github.com/Yesterday17/iori/tree/shiori-v0.1.1
[0.1.3]: https://github.com/Yesterday17/iori/tree/shiori-v0.1.3
[0.1.4]: https://github.com/Yesterday17/iori/tree/shiori-v0.1.4
[0.2.0]: https://github.com/Yesterday17/iori/tree/shiori-v0.2.0
[0.2.1]: https://github.com/Yesterday17/iori/tree/shiori-v0.2.1
[0.2.2]: https://github.com/Yesterday17/iori/tree/shiori-v0.2.2
