#!/usr/bin/env bash
#
# Films HyperLab, and turns the recording into an mp4 and a gif.
#
#   GROQ_API_KEY=... apps/desktop/demo/record.sh
#
# Starts the two things the film needs — the Vite dev server for the
# interface, and hyperlab-bridge for a real runtime behind it — drives them
# with Playwright, and stops them again.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app="$(cd "$here/.." && pwd)"
root="$(cd "$app/../.." && pwd)"
out="$root/target/demo"

: "${GROQ_API_KEY:?set GROQ_API_KEY to a Groq key before filming}"
: "${GROQ_MODEL:=openai/gpt-oss-120b}"
export GROQ_API_KEY GROQ_MODEL

stack="${1:-$root/examples/Recipe Box.hl}"
ffmpeg="${FFMPEG:-$(command -v ffmpeg || echo /opt/pw-browsers/ffmpeg-1011/ffmpeg-linux)}"

rm -rf "$out"
mkdir -p "$out"

cleanup() {
  [[ -n "${bridge_pid:-}" ]] && kill "$bridge_pid" 2>/dev/null || true
  [[ -n "${vite_pid:-}" ]] && kill "$vite_pid" 2>/dev/null || true
}
trap cleanup EXIT

echo "building the bridge…"
cargo build --manifest-path "$app/src-tauri/Cargo.toml" --bin hyperlab-bridge --quiet

echo "starting the bridge on :7878 with $stack"
"$app/src-tauri/target/debug/hyperlab-bridge" --port 7878 --stack "$stack" &
bridge_pid=$!

echo "starting the interface on :5173"
(cd "$app" && npx vite --port 5173 --strictPort >"$out/vite.log" 2>&1) &
vite_pid=$!

# Both are local, so they come up in a moment or not at all.
for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:5173" >/dev/null 2>&1 &&
    curl -fsS "http://127.0.0.1:7878/events" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

echo "filming…"
(cd "$app" && node demo/film.mjs)

webm="$(find "$out" -name '*.webm' -print -quit)"
[[ -n "$webm" ]] || {
  echo "no recording was produced" >&2
  exit 1
}

echo "converting…"
# yuv420p and an even width keep the mp4 playable everywhere, including in a
# browser and in a pull request.
"$ffmpeg" -y -loglevel error -i "$webm" \
  -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2,fps=24" \
  -c:v libx264 -pix_fmt yuv420p -crf 24 -movflags +faststart \
  "$out/hyperlab.mp4"

# The gif is a highlight, not the whole film: the assistant act, which is the
# part worth putting in a README. All ninety seconds at a legible size would
# be twenty megabytes, and nobody scrolls past that.
#
# GIF_FROM and GIF_FOR move the window if the film changes length.
: "${GIF_FROM:=66}"
: "${GIF_FOR:=26}"
: "${GIF_WIDTH:=640}"

# A shared palette, or a black-and-white interface dithers into mush — and a
# small one, because Neo Classic is very nearly monochrome. Thirty-two
# colours is indistinguishable here and half the bytes, and with that few
# there is nothing left to dither.
"$ffmpeg" -y -loglevel error -ss "$GIF_FROM" -t "$GIF_FOR" -i "$webm" \
  -vf "fps=10,scale=$GIF_WIDTH:-1:flags=lanczos,palettegen=max_colors=32:stats_mode=diff" \
  "$out/palette.png"
"$ffmpeg" -y -loglevel error -ss "$GIF_FROM" -t "$GIF_FOR" -i "$webm" -i "$out/palette.png" \
  -lavfi "fps=10,scale=$GIF_WIDTH:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=none" \
  "$out/hyperlab.gif"
rm -f "$out/palette.png"

echo
echo "  $out/hyperlab.mp4"
echo "  $out/hyperlab.gif"
