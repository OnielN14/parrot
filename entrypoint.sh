#!/bin/sh

ARCH=$(uname -m)

# Convert aarch64 to arm64 and x86_64 to amd64 for consistency
if [ "$ARCH" = "aarch64" ]; then
    ARCH="arm64"
elif [ "$ARCH" = "x86_64" ]; then
    ARCH="amd64"
fi

echo "Running on architecture: $ARCH"


if [ "$ARCH" = "arm64" ]; then
    # Commands specific to ARM
    YTDLP_LINK=https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64
elif [ "$ARCH" = "amd64" ]; then
    # Commands specific to x86/AMD64
    YTDLP_LINK=https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux
fi


wget --no-check-certificate $YTDLP_LINK -O /usr/local/bin/yt-dlp

chmod a+rx /usr/local/bin/yt-dlp

parrot