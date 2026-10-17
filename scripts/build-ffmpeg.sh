#!/usr/bin/env bash
# Builds the FFmpeg that Kadr ships on macOS and Linux: LGPL only, ffmpeg +
# ffprobe, all native decoders and demuxers (HEIC needs FFmpeg >= 7.1),
# dav1d for AVIF, PNG/MJPEG encoders for thumbnails, no network or devices.
#
#   scripts/build-ffmpeg.sh <out-dir> [arch]
#
# arch: native (default), or x86_64 / arm64 to cross-build on macOS.
# Produces <out-dir>/ffmpeg, <out-dir>/ffprobe and LICENSE/README files.
set -euo pipefail

FFMPEG_VERSION="${FFMPEG_VERSION:-8.1.3}"
DAV1D_VERSION="${DAV1D_VERSION:-1.5.4}"
OUT="$(mkdir -p "$1" && cd "$1" && pwd)"
ARCH="${2:-native}"
WORK="$(mktemp -d)"
PREFIX="$WORK/prefix"
OS="$(uname -s)"
JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu)"

cd "$WORK"
curl -fsSL "https://downloads.videolan.org/pub/videolan/dav1d/$DAV1D_VERSION/dav1d-$DAV1D_VERSION.tar.xz" | tar xJ
curl -fsSL "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" | tar xJ

# --- dav1d (BSD-2) -----------------------------------------------------------
MESON_CROSS=()
if [[ "$OS" == Darwin && "$ARCH" != native ]]; then
  cpu=$([[ "$ARCH" == arm64 ]] && echo aarch64 || echo x86_64)
  cat > cross.txt <<CROSS
[binaries]
c = ['clang', '-arch', '$ARCH']
ar = 'ar'
strip = 'strip'
[host_machine]
system = 'darwin'
cpu_family = '$cpu'
cpu = '$cpu'
endian = 'little'
CROSS
  MESON_CROSS=(--cross-file "$WORK/cross.txt")
fi
meson setup dav1d-build "dav1d-$DAV1D_VERSION" "${MESON_CROSS[@]}" \
  --prefix="$PREFIX" --libdir=lib --default-library=static --buildtype=release \
  -Denable_tools=false -Denable_tests=false ${MACOSX_DEPLOYMENT_TARGET:+-Dc_args=-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET -Dc_link_args=-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET}
ninja -C dav1d-build install

# --- FFmpeg (LGPL 2.1+) ------------------------------------------------------
FF_CROSS=()
if [[ "$OS" == Darwin && "$ARCH" != native ]]; then
  FF_CROSS=(--enable-cross-compile --target-os=darwin --arch="$ARCH" --cc="clang -arch $ARCH")
fi
EXTRA_LIBS=()
if [[ "$OS" == Linux ]]; then
  # Fully static: runs on any glibc-based distribution.
  EXTRA_LIBS=(--extra-ldflags="-static" --pkg-config-flags="--static")
else
  EXTRA_LIBS=(--pkg-config-flags="--static")
fi
cd "ffmpeg-$FFMPEG_VERSION"
PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig" ./configure \
  --prefix="$PREFIX" "${FF_CROSS[@]}" "${EXTRA_LIBS[@]}" \
  --disable-shared --enable-static --enable-pic \
  --disable-doc --disable-debug --disable-ffplay --disable-network \
  --disable-autodetect --disable-devices --disable-indevs --disable-outdevs \
  --enable-zlib --enable-libdav1d \
  --disable-encoders --enable-encoder=png,mjpeg \
  --disable-muxers --enable-muxer=image2,image2pipe,null \
  --extra-version=kadr
make -j"$JOBS"
cp ffmpeg ffprobe "$OUT/"
cp COPYING.LGPLv2.1 "$OUT/LICENSE.txt"
cat > "$OUT/README.txt" <<README
FFmpeg $FFMPEG_VERSION built for Kadr (LGPL 2.1 or later; no GPL or nonfree parts).
Source: https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz
dav1d $DAV1D_VERSION (BSD-2-Clause): https://code.videolan.org/videolan/dav1d
Build script: scripts/build-ffmpeg.sh in https://github.com/henzelis/kadr-ai
README
"$OUT/ffprobe" -hide_banner -version | head -1 || true
