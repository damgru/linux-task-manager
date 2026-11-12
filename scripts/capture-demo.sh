#!/bin/sh
# Capture public release media from fictional, read-only English demo fixtures.
set -eu
cd "$(dirname "$0")/.."
repo=$(pwd -P)
out=${1:?Specify an output directory outside the checkout}
mkdir -p "$out"
out=$(realpath "$out")
case "$out/" in "$repo/"*) echo "Demo media must be outside the checkout" >&2; exit 2;; esac
binary=${BINARY:-./target/release/linux-task-manager}
command -v ffmpeg >/dev/null
capture() {
    "$binary" --demo --language en "$@" --screenshot "$out/frame-$index.png"
    index=$((index + 1))
}
index=10
capture --page apps
capture --page processes
for resource in cpu memory disk network gpu; do
    capture --page performance --resource "$resource"
done
for page in app-history startup-apps users details services settings; do
    capture --page "$page"
done
capture --application 200
for view in projects containers images; do
    capture --page docker --docker-view "$view"
done
capture --page docker --docker-project workspace
# Three seconds per view; retain enough width for readable headings on GitHub.
ffmpeg -y -framerate 1/3 -start_number 10 -i "$out/frame-%d.png" \
    -filter_complex '[0:v]scale=1200:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse=dither=bayer' \
    -loop 0 "$out/demo-tour.gif"
printf 'Demo tour saved to %s/demo-tour.gif\n' "$out"
