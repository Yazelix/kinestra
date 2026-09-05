#!/usr/bin/env bash
# Sourced by the packaged kinestra command before the consumer's Bash scenario.
kinestra_work=""
kinestra_display_pid=""
kinestra_compositor_pid=""
kinestra_app_pid=""
kinestra_capture_pid=""

kinestra_cleanup() {
	local status=$? pid
	trap - EXIT
	if [[ -n "$kinestra_capture_pid" ]]; then
		kill -INT "$kinestra_capture_pid" 2>/dev/null || true
		wait "$kinestra_capture_pid" 2>/dev/null || true
	fi
	if declare -F kinestra_cleanup_hook >/dev/null; then
		kinestra_cleanup_hook || true
	fi
	for pid in "$kinestra_app_pid" "$kinestra_compositor_pid" "$kinestra_display_pid"; do
		if [[ -n "$pid" ]]; then
			kill "$pid" 2>/dev/null || true
			wait "$pid" 2>/dev/null || true
		fi
	done
	if [[ -n "$kinestra_work" ]]; then
		if ((status == 0)); then
			rm -r -- "$kinestra_work"
		else
			printf 'Kinestra logs: %s\n' "$kinestra_work" >&2
		fi
	fi
	exit "$status"
}

kinestra_display() {
	kinestra_width=$1
	kinestra_height=$2
	[[ -z "$kinestra_work" && "$1" =~ ^[1-9][0-9]*$ && "$2" =~ ^[1-9][0-9]*$ ]] || return 2
	((kinestra_width % 2 == 0 && kinestra_height % 2 == 0)) || return 2
	kinestra_work=$(mktemp -d -t kinestra.XXXXXX)
	Xvfb -displayfd 3 -screen 0 "${1}x${2}x24" -nolisten tcp +extension Composite \
		3>"$kinestra_work/display" >"$kinestra_work/xvfb.log" 2>&1 &
	kinestra_display_pid=$!
	local attempt
	for ((attempt = 0; attempt < 100; attempt++)); do
		[[ -s "$kinestra_work/display" ]] && break
		kill -0 "$kinestra_display_pid" 2>/dev/null || return 1
		sleep 0.1
	done
	[[ -s "$kinestra_work/display" ]] || return 1
	DISPLAY=":$(<"$kinestra_work/display")"
	export DISPLAY WINIT_UNIX_BACKEND=x11 TERM=xterm-256color
	unset WAYLAND_DISPLAY XDG_SESSION_TYPE XDG_CURRENT_DESKTOP NO_COLOR
	if [[ -n "${3:-}" ]]; then
		xwallpaper --zoom "$3"
		picom --backend xrender --vsync --config /dev/null >"$kinestra_work/picom.log" 2>&1 &
		kinestra_compositor_pid=$!
		sleep 1
		kill -0 "$kinestra_compositor_pid"
	fi
}

kinestra_stop_app() {
	if [[ -n "$kinestra_app_pid" ]]; then
		kill "$kinestra_app_pid" 2>/dev/null || true
		wait "$kinestra_app_pid" 2>/dev/null || true
		kinestra_app_pid=""
	fi
}

kinestra_launch() {
	local class=$1 window="" attempt
	shift
	kinestra_stop_app
	"$@" >"$kinestra_work/app.log" 2>&1 &
	kinestra_app_pid=$!
	for ((attempt = 0; attempt < 200; attempt++)); do
		window=$(xdotool search --class "$class" 2>/dev/null | head -n1 || true)
		[[ -n "$window" ]] && break
		kill -0 "$kinestra_app_pid" 2>/dev/null || break
		sleep 0.1
	done
	if [[ -z "$window" ]]; then
		cat "$kinestra_work/app.log" >&2
		return 1
	fi
	xdotool windowmove "$window" 0 0 windowsize "$window" "$kinestra_width" "$kinestra_height" windowfocus --sync "$window"
}

kinestra_record() {
	local output=$1 attempt status=0
	shift
	rm -f -- "$kinestra_work/progress"
	ffmpeg -hide_banner -loglevel error -nostdin \
		-f x11grab -draw_mouse 0 -framerate 30 -video_size "${kinestra_width}x${kinestra_height}" -i "$DISPLAY" \
		-an -c:v libx264 -crf 18 -preset veryfast -pix_fmt yuv420p -movflags +faststart \
		-stats_period 0.1 -progress "$kinestra_work/progress" -y "$output" >"$kinestra_work/ffmpeg.log" 2>&1 &
	kinestra_capture_pid=$!
	for ((attempt = 0; attempt < 100; attempt++)); do
		[[ -s "$kinestra_work/progress" ]] && break
		kill -0 "$kinestra_capture_pid" 2>/dev/null || break
		sleep 0.1
	done
	if [[ ! -s "$kinestra_work/progress" ]]; then
		cat "$kinestra_work/ffmpeg.log" >&2
		return 1
	fi
	"$@"
	kill -INT "$kinestra_capture_pid"
	wait "$kinestra_capture_pid" || status=$?
	kinestra_capture_pid=""
	# FFmpeg reports 255 for the intentional SIGINT that finalizes the MP4.
	[[ "$status" == 0 || "$status" == 255 ]] || return "$status"
	ffprobe -v error -show_entries format=duration -of csv=p=0 "$output"
}

kinestra_snapshot() {
	ffmpeg -hide_banner -loglevel error -nostdin -f x11grab -draw_mouse 0 \
		-video_size "${kinestra_width}x${kinestra_height}" -i "$DISPLAY" -frames:v 1 -y "$1"
}

kinestra_poster() {
	ffmpeg -hide_banner -loglevel error -nostdin -ss "$2" -i "$1" -frames:v 1 -y "$3"
}

kinestra_gif() {
	ffmpeg -hide_banner -loglevel error -nostdin -i "$1" \
		-filter_complex "fps=${4:-15},scale=${3:-960}:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse=dither=bayer" \
		-loop 0 -y "$2"
}
