#!/usr/bin/env bash
# A private, headless Wayland display per worktree, for driving the desktop app
# without touching the user's screen: sway (WLR_BACKENDS=headless), a pointer
# and a keyboard held by tools/headless-input.py (clicks go through sway's own
# cursor commands) and grim for screenshots. The tools come from `nix develop`,
# or from nixpkgs on the fly when they're not on PATH.
#
#   tools/headless.sh start [WIDTHxHEIGHT]   start this worktree's display
#   tools/headless.sh run [VAR=val…] CMD …   run CMD on it (background, logs to state dir)
#   tools/headless.sh click X Y [right|middle]
#   tools/headless.sh type TEXT              type text into the focused window
#   tools/headless.sh key NAME…              press keys (Tab, Return, ctrl+s, …)
#   tools/headless.sh hold KEY SECONDS       hold one key (at most 10 seconds)
#   tools/headless.sh wheel vertical|horizontal AMOUNT COUNT GAP
#   tools/headless.sh move X Y               move within the output
#   tools/headless.sh press|release          left pointer button for dragging
#   tools/headless.sh shot FILE.png          screenshot
#   tools/headless.sh stop                   stop the apps it started and the display
#
# State (env, PIDs, logs, and sway's runtime dir with its sockets) lives in
# $HEADLESS_DIR, default <worktree>/target/headless.
set -euo pipefail

root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
dir=${HEADLESS_DIR:-$root/target/headless}
mkdir -p "$dir"

tool() {
  if command -v "$1" >/dev/null; then "$@"
  else
    local pkg=$1; [[ $1 == swaymsg ]] && pkg=sway
    nix --extra-experimental-features 'nix-command flakes' shell "nixpkgs#$pkg" -c "$@"
  fi
}

load() {
  [[ -f $dir/env ]] || { echo "no display; run: $0 start" >&2; exit 1; }
  # shellcheck disable=SC1091
  . "$dir/env"; export WAYLAND_DISPLAY SWAYSOCK XDG_RUNTIME_DIR
}

# A clean environment: the caller's (often a KDE session's) variables break GTK
# on another compositor (P-099).
clean_env() {
  exec env -i HOME="$HOME" PATH="$PATH" USER="$USER" LANG="${LANG:-C.UTF-8}" \
    XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" \
    WAYLAND_DISPLAY="$WAYLAND_DISPLAY" XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland "$@"
}

case ${1:-} in
  start)
    if [[ -f $dir/sway.pid ]] && kill -0 "$(cat "$dir/sway.pid")" 2>/dev/null; then
      cat "$dir/env"; exit 0
    fi
    size=${2:-1600x1000}
    rm -f "$dir/env"
    cat > "$dir/sway.config" <<EOF
output HEADLESS-1 resolution $size
xwayland disable
default_border none
focus_follows_mouse no
exec sh -c 'printf "WAYLAND_DISPLAY=%s\nSWAYSOCK=%s\nXDG_RUNTIME_DIR=%s\n" "\$WAYLAND_DISPLAY" "\$SWAYSOCK" "\$XDG_RUNTIME_DIR" > "$dir/env.tmp" && mv "$dir/env.tmp" "$dir/env"'
EOF
    # A private runtime dir: sway's sockets stay inside the worktree (no
    # litter in /run/user, and it works in sandboxes that can write only there).
    rt=$dir/run; mkdir -p "$rt"; chmod 700 "$rt"
    WAYLAND_DISPLAY='' XDG_RUNTIME_DIR=$rt clean_env WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=${HEADLESS_RENDERER:-pixman} \
      "$(command -v sway || echo sway)" ${HEADLESS_SWAY_ARGS:-} -c "$dir/sway.config" > "$dir/sway.log" 2>&1 &
    echo $! > "$dir/sway.pid"
    for _ in $(seq 50); do [[ -f $dir/env ]] && break; sleep 0.1; done
    [[ -f $dir/env ]] || { echo "sway didn't start; see $dir/sway.log" >&2; exit 1; }
    load
    python3 -I "$root/tools/headless-input.py" serve "$dir/input.sock" > "$dir/input.log" 2>&1 &
    echo $! > "$dir/input.pid"
    for _ in $(seq 30); do grep -q attached "$dir/input.log" 2>/dev/null && break; sleep 0.1; done
    grep -q attached "$dir/input.log" || { cat "$dir/input.log" >&2; exit 1; }
    cat "$dir/env"
    ;;
  run)
    shift; load
    name=app; for a in "$@"; do [[ $a == *=* ]] || { name=$(basename "$a"); break; }; done
    # WebKit's GPU path crashes in libnvidia-eglcore on a headless sway once a
    # pointer is attached, so apps get Mesa's software GL (HEADLESS_GPU=1 opts out).
    gl=(); [[ ${HEADLESS_GPU:-0} == 1 ]] || gl=(__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json LIBGL_ALWAYS_SOFTWARE=1)
    case ${HEADLESS_GL_THREADS:-} in
      "") ;;
      [0-9]|[12][0-9]|3[0-2]) gl+=(LP_NUM_THREADS="$HEADLESS_GL_THREADS") ;;
      *) echo "HEADLESS_GL_THREADS must be an integer in [0, 32]" >&2; exit 2 ;;
    esac
    clean_env LANG="${LANG:-C.UTF-8}" "${gl[@]}" "$@" > "$dir/$name.log" 2>&1 &
    echo $! >> "$dir/apps.pid"
    echo "pid $! · log $dir/$name.log"
    ;;
  click)
    load
    button=button1; [[ ${4:-} == middle ]] && button=button2; [[ ${4:-} == right ]] && button=button3
    tool swaymsg -q seat - cursor set "$2" "$3"
    tool swaymsg -q seat - cursor press "$button"
    tool swaymsg -q seat - cursor release "$button"
    ;;
  hold|wheel)
    load; input_action=$1; shift
    python3 -I "$root/tools/headless-input.py" send "$dir/input.sock" "$input_action" "$@"
    ;;
  move)
    load
    size=$(awk '/^output HEADLESS-1 resolution / {print $4; exit}' "$dir/sway.config")
    python3 -I "$root/tools/headless-input.py" send "$dir/input.sock" motion "$2" "$3" "${size%x*}" "${size#*x}"
    ;;
  press|release)
    load; python3 -I "$root/tools/headless-input.py" send "$dir/input.sock" button "$1"
    ;;
  type)
    load; shift; python3 -I "$root/tools/headless-input.py" send "$dir/input.sock" type "$*"
    ;;
  key)
    load; shift; python3 -I "$root/tools/headless-input.py" send "$dir/input.sock" key "$@"
    ;;
  shot)
    load; tool grim "${2:?file}"
    ;;
  stop)
    if [[ -f $dir/apps.pid ]]; then xargs -r kill < "$dir/apps.pid" 2>/dev/null || true; rm -f "$dir/apps.pid"; fi
    for f in input sway; do
      if [[ -f $dir/$f.pid ]]; then kill "$(cat "$dir/$f.pid")" 2>/dev/null || true; rm -f "$dir/$f.pid"; fi
    done
    rm -f "$dir/env"
    ;;
  *)
    sed -n '2,15p' "$0"; exit 2
    ;;
esac
