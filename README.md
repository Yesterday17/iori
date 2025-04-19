# iori

A brand new HLS / MPEG-Dash stream downloader.

## Download

You can get the pre-compiled executable files from `artifacts`. (Like [this](https://github.com/Yesterday17/iori/actions/runs/11423831843))

## Project Structure

- `bin`: Contains the main executable crates, like `minyami` and `shiori`.
- `crates`: Core library crates, such as `iori` (the downloader core) and `iori-ssa` (Sample-AES decryption).
- `plugins`: Plugin system related crates, like `shiori-plugin`.
- `platforms`: Video platform-specific implementations, such as `iori-nicolive` and `iori-showroom`.

## Road to 1.0

- [ ] Separate decrypt and download