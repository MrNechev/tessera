#!/usr/bin/env bash
# Generates a small synthetic media library for the contract test (no real media).
#   gen_library.sh <out-dir>
# Uses ffmpeg/vips from the letopis:0.3.0 image (x264/x265/vpx) and exiftool/vips from the
# tessera-dev distrobox for EXIF tags and HEIC.
set -euo pipefail
OUT=$(realpath -m "$1")
IMAGE=${LETOPIS_IMAGE:-letopis:0.3.0}
rm -rf "$OUT"
mkdir -p "$OUT/trips/iceland" "$OUT/Смешни" "$OUT/live" "$OUT/music" "$OUT/.hidden"

docker run --rm --user "$(id -u):$(id -g)" -v "$OUT:/out" --entrypoint sh "$IMAGE" -c '
set -e
cd /out
ff="ffmpeg -v error -nostdin -y"
# images
$ff -f lavfi -i testsrc2=s=1600x1200 -frames:v 1 -q:v 3 trips/iceland/reykjavik.jpg
$ff -f lavfi -i testsrc2=s=1200x1600 -frames:v 1 -q:v 3 trips/portrait.jpg
$ff -f lavfi -i mandelbrot=s=800x600 -frames:v 1 trips/fractal.png
$ff -f lavfi -i "mandelbrot=s=800x600,eq=brightness=0.02" -frames:v 1 -q:v 4 trips/fractal-copy.jpg
$ff -f lavfi -i smptebars=s=640x360 -frames:v 1 bars.webp
$ff -f lavfi -i "testsrc=s=320x240:d=2:r=10" -vf "split[a][b];[a]palettegen[p];[b][p]paletteuse" Смешни/котка.gif
$ff -f lavfi -i color=c=red:s=1x1 -frames:v 1 spacer.gif
# videos
$ff -f lavfi -i testsrc2=s=1280x720:d=4:r=25 -f lavfi -i sine=f=440:d=4 -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest -metadata creation_time=2023-05-06T07:08:09Z trips/iceland/drive.mp4
$ff -f lavfi -i testsrc2=s=1920x1080:d=3:r=25 -c:v libx265 -tag:v hvc1 -pix_fmt yuv420p -an -movflags use_metadata_tags -metadata location=+64.1466-021.9426/ trips/iceland/hevc.mp4
$ff -f lavfi -i testsrc2=s=640x360:d=3:r=25 -c:v libvpx-vp9 -b:v 300k -an clip.webm
$ff -f lavfi -i testsrc2=s=640x480:d=2:r=25 -c:v libx264 -pix_fmt yuv420p -an old.mkv
$ff -f lavfi -i testsrc2=s=2560x1440:d=2:r=25 -c:v libx264 -pix_fmt yuv420p -an big1440.mp4
$ff -f lavfi -i testsrc2=s=640x480:d=2:r=25 -c:v libx264 -pix_fmt yuv422p10le -an tenbit.mov
# live photo pair (image + short .mov with the same name)
$ff -f lavfi -i testsrc2=s=1024x768 -frames:v 1 -q:v 3 live/IMG_0001.jpg
$ff -f lavfi -i testsrc2=s=1024x768:d=2:r=25 -c:v libx264 -pix_fmt yuv420p -an live/IMG_0001.mov
# audio
$ff -f lavfi -i sine=f=330:d=3 -c:a libmp3lame music/tone.mp3
# duplicate + hidden + non-media
cp trips/iceland/reykjavik.jpg trips/reykjavik-copy.jpg
cp bars.webp .hidden/secret.webp
echo "not media" > notes.txt
'
# EXIF: capture date, camera and GPS
distrobox enter tessera-dev -- bash -lc "
set -e
cd '$OUT'
exiftool -q -overwrite_original -Make=Apple -Model='iPhone 15 Pro' -DateTimeOriginal='2024:07:14 18:22:05' -OffsetTimeOriginal=+02:00 \
  -GPSLatitude=64.1466 -GPSLatitudeRef=N -GPSLongitude=21.9426 -GPSLongitudeRef=W trips/iceland/reykjavik.jpg trips/reykjavik-copy.jpg
exiftool -q -overwrite_original -Make='NIKON CORPORATION' -Model='NIKON Z 6' -DateTimeOriginal='2022:01:02 03:04:05' -Orientation#=6 \
  -GPSLatitude=42.6977 -GPSLatitudeRef=N -GPSLongitude=23.3219 -GPSLongitudeRef=E trips/portrait.jpg
exiftool -q -overwrite_original -Make=Canon -Model='EOS R5' -DateTimeOriginal='2021:12:24 20:00:00' trips/fractal-copy.jpg
# HEIC (or AVIF if no HEVC encoder is available)
if vips copy trips/fractal.png 'trips/fractal.heic[compression=hevc]' 2>/dev/null; then :; else rm -f trips/fractal.heic; vips copy trips/fractal.png 'trips/fractal.avif[compression=av1]'; fi
exiftool -q -overwrite_original -Make=Apple -Model='iPhone 13' -DateTimeOriginal='2020:02:02 12:00:00' trips/fractal.heic 2>/dev/null || true
"
# stable mtimes (the FileKey includes the mtime)
find "$OUT" -type f -exec touch -d '2025-01-01 12:00:00' {} +
touch -d '2025-03-01 08:00:00' "$OUT/trips/iceland/drive.mp4"
find "$OUT" -type f | sort
