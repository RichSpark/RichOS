#!/bin/sh
# glance-saved-note.sh — run IN THE GUEST: give an image a saved RichCleaner note, so RichGlance's
#                        info overlay has a note to show without RichCleaner filing anything.
#
#   sh glance-saved-note.sh <image> <note-file>
#
# Push it with a steps-walk `push` step and run it with a `guest` step. It adds one filed record
# for <image> to the guest user's RichCleaner catalog (created when missing), in the shape
# RichGlance reads (richos/tools/richfiles/Sources/RichFilesCore/GlanceMetadata.swift): the file's
# volume, object, size and modification time as they are at this moment. The note is the first
# line of <note-file>; a quote or a backslash in it, or a quote in the path, is refused.
set -eu
[ "$#" -eq 2 ] || { echo "usage: glance-saved-note.sh <image> <note-file>" >&2; exit 2; }
[ -f "$1" ] && [ -f "$2" ] || { echo "glance-saved-note.sh: $1 or $2 is not a file" >&2; exit 2; }
image="$(cd "$(dirname "$1")" && pwd -P)/$(basename "$1")"
note="$(head -n 1 "$2")"
case "$note$image" in
  *\'*|*\"*|*\\*) echo "glance-saved-note.sh: a quote or backslash in the note or path is not supported" >&2; exit 2 ;;
esac
volume=$(( $(stat -f %d "$image") & 0xFFFFFFFF ))
object="$(stat -f %i "$image")"
size="$(stat -f %z "$image")"
modified="$(stat -f %Fm "$image")"
seconds="${modified%.*}"
nanoseconds="$(printf '%s' "${modified#*.}" | sed 's/^0*//')"
catalog="$HOME/Library/Application Support/com.richos.app/RichCleaner/library.sqlite"
mkdir -p "$(dirname "$catalog")"
record="{\"path\":\"$image\",\"identity\":{\"volume\":$volume,\"object\":$object,\"size\":$size,\"modified\":\"$seconds:${nanoseconds:-0}\"},\"note\":\"$note\",\"labels\":[],\"folder\":\"\",\"updated\":$(date +%s)000,\"projection\":{\"mode\":\"sidecar\"}}"
sqlite3 "$catalog" "CREATE TABLE IF NOT EXISTS files (volume TEXT, object TEXT, phase TEXT, record TEXT);
INSERT INTO files VALUES ('$volume', '$object', '\"filed\"', '$record');"
echo "saved a note of ${#note} characters for $image (volume $volume, object $object, modified $seconds:${nanoseconds:-0})"
