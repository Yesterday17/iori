#!/bin/bash
ffmpeg -f lavfi -i color=c=black:s=1920x1080:d=2 -f lavfi -i "sine=frequency=1000:duration=2" -c:a ac3 -b:a 192k output.mp4
mp42hls --encryption-mode SAMPLE-AES --encryption-key a8cda0ee5390b716298ffad0a1f1a021E60C79C314E3C9B471E7E51ABAA0B24A --encryption-iv-mode fps output.mp4
rm *.mp4 *.m3u8
ssadecrypt --key a8cda0ee5390b716298ffad0a1f1a021 --iv E60C79C314E3C9B471E7E51ABAA0B24A segment-0.ts | ffplay -