# iori

A brand new HLS / MPEG-Dash stream downloader.

## Download

You can get the pre-compiled executable files from `artifacts`. (Like [this](https://github.com/Yesterday17/iori/actions/runs/11423831843))

## iori-minyami

`iori-minyami` is a cli-tool compatible with [`minyami`](https://github.com/Last-Order/Minyami). In addition, it supports `mp4decrypt` and `MPEG-DASH`(Partial).

```text
Usage: minyami [OPTIONS] <M3U8>

Arguments:
  <M3U8>
          m3u8 file path

Options:
      --verbose
          Debug output

      --threads <THREADS>
          Threads limit
          
          [default: 5]

      --retries <RETRIES>
          Retry limit
          
          [default: 5]

  -o, --output <OUTPUT>
          [Unimplemented] Output file path
          
          [default: ./output.mkv]

      --temp-dir <TEMP_DIR>
          Temporary file path
          
          [env: TEMP=]

      --key <KEY>
          Set key manually (Internal use)
          
          (Optional) Key for decrypt video.

      --cookies <COOKIES>
          Cookies used to download

  -H, --headers <HEADERS>
          HTTP Header used to download
          
          Custom header. eg. "User-Agent: xxxxx". This option will override --cookies.

      --live
          Download live

      --format <FORMAT>
          [Unimplemented] (Optional) Set output format. default: ts Format name. ts or mkv

      --proxy <PROXY>
          [Unimplemented] Use the specified HTTP/HTTPS/SOCKS5 proxy
          
          Set proxy in [protocol://<host>:<port>] format. eg. --proxy "http://127.0.0.1:1080".

      --slice <SLICE>
          [Unimplemented] Download specified part of the stream
          
          Set time range in [<hh:mm:ss>-<hh:mm:ss> format]. eg. --slice "45:00-53:00"

      --no-merge
          Do not merge m3u8 chunks

  -k, --keep
          Keep temporary files

      --keep-encrypted-chunks
          [Unimplemented] Do not delete encrypted chunks after decryption

      --chunk-naming-strategy <CHUNK_NAMING_STRATEGY>
          [Unimplemented] Temporary file naming strategy. Defaults to 1.
          
          MIXED = 0, USE_FILE_SEQUENCE = 1, USE_FILE_PATH = 2,
          
          [default: 1]

      --range <RANGE>
          [Iori Argument] Specify segment range to download in archive mode
          
          [default: -]

      --resume-dir <RESUME_DIR>
          [Iori Argument] Specify the resume folder path

      --pipe
          [Iori Argument] Pipe live streaming to stdout. Only takes effect in live mode

      --dash
          [Iori Argument] Download with dash format

  -h, --help
          Print help (see a summary with '-h')
```

## Road to stable

- [ ] Separate decrypt and download