# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1]

### Fixed

- Decrypted MPEG-TS segments now have the correct `continuity counter` and pass the [continuity check](https://github.com/FFmpeg/FFmpeg/blob/43be8d07281caca2e88bfd8ee2333633e1fb1a13/libavformat/mpegts.c#L2826-L2828).
- Resolved some `clone` operations.

## [0.1.0] - 2025-02-01

### Added

- `Sample-AES` decryption support.
