#!/usr/bin/env bash
# colors.sh SHOT.png X,Y W H: the most common non-background colors of a
# region of a screenshot (ImageMagick), to tell text colors apart.
img="$1"; at="$2"; w="${3:-60}"; h="${4:-14}"
x="${at%,*}"; y="${at#*,}"
convert "$img" -crop "${w}x${h}+${x}+${y}" +repage -format %c histogram:info:- | sort -rn | head -6
