# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-05-05

### Breaking Changes

- Changed the environment variable to control `temp_dir` form `TEMP` to `TEMP_DIR`.
- Updated inspector argument input. Now you should use the following arguments directly instead of using `-e/--args`:
  - `nico-user-session`
  - `nico-download-danmaku`
  - `nico-chase-play`
  - `showroom-user-session`

### New Features

- **Nicolive**: Added `--nico-chase-play` to download nico live from start.
- **Gigafile**: Experimental support for downloading file from [Gigafile](https://gigafile.nu/).

### Updated

- Changed default temp dir to `current_dir` instead of `temp_dir`.
- Added some i18n for command line options.
- Segments from different streams will be mixed before download. This makes `--pipe-mux` available to play vods.
- **Nicolive**: Added `frontend_id` to `webSocketUrl` to match the behavior of web.

### Fixed

- Fixed panic on error occurs when using `--wait` in `shiori download`.
- Fixed pipe output.

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
