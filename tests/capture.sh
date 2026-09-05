#!/usr/bin/env bash
# shellcheck disable=SC2154
# Executed by kinestra; helpers and bookkeeping are provided by its launcher.
set -euo pipefail

if [[ $# != 0 ]]; then
	kinestra_display 320 180
	printf '%s\n' "$kinestra_display_pid" >"$2/display-pid"
	printf '%s\n' "$kinestra_work" >"$2/log-dir"
	kinestra_record "$2/interrupted.mp4" "$1"
	exit 99
fi

kinestra_display 320 180
kinestra_record "$kinestra_work/test.mp4" sleep 0.5
kinestra_poster "$kinestra_work/test.mp4" 0 "$kinestra_work/poster.png"
kinestra_gif "$kinestra_work/test.mp4" "$kinestra_work/test.gif" 160 10
[[ $(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$kinestra_work/test.gif") == 160,90 ]]
[[ -s "$kinestra_work/poster.png" ]]

interrupt() { kill -TERM "$$"; }
export -f interrupt
for action in false interrupt; do
	status=0
	kinestra "$CHECK_SCENARIO" "$action" "$kinestra_work" || status=$?
	if [[ $action == false ]]; then [[ $status == 1 ]]; else [[ $status == 143 ]]; fi
	if kill -0 "$(<"$kinestra_work/display-pid")" 2>/dev/null; then exit 1; fi
	ffprobe -v error "$kinestra_work/interrupted.mp4"
	rm -r -- "$(<"$kinestra_work/log-dir")"
done
