#!/bin/sh
# A stand-in for the `claude` CLI that speaks its stream-json protocol (the shapes
# `claude` 2.1.278 emits), for tests: `claude -p <prompt> --output-format stream-json …`.
# Replies "mock claude: <prompt>" after one pretend Read of README.md; `--resume` is
# acknowledged so session continuity can be checked. a prompt containing MOCK_FAIL exits with an error result.
# Milestone 15: `--version` and `auth status --json` (logged in unless MOCK_CLAUDE_LOGGED_OUT is set).
case "$1" in
  --version) echo "9.9.9 (Claude Code)"; exit 0 ;;
  auth) if [ "$2" = status ]; then
          if [ -n "$MOCK_CLAUDE_LOGGED_OUT" ]; then echo '{"loggedIn":false}'; else echo '{"loggedIn":true,"authMethod":"claude.ai","email":"mock@example.org"}'; fi
        else echo "mock login: open https://claude.ai/login and paste the code"; fi
        exit 0 ;;
esac
prompt=""; session=""; resumed=no; mode=""
while [ $# -gt 0 ]; do
  case "$1" in
    -p) prompt="$2"; shift 2 ;;
    --session-id) session="$2"; shift 2 ;;
    --resume) session="$2"; resumed=yes; shift 2 ;;
    --permission-mode) mode="$2"; shift 2 ;;
    --output-format|--model|--allowedTools) shift 2 ;;
    *) shift ;;
  esac
done
[ -n "$session" ] || session="mock-session"
esc() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g' | tr '\n' ' '; }
printf '{"type":"system","subtype":"init","cwd":"%s","session_id":"%s","model":"mock-claude","permissionMode":"%s"}\n' "$(esc "$PWD")" "$session" "$mode"
printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"README.md"}}]},"session_id":"%s"}\n' "$session"
printf '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"# Sample","is_error":false}]},"session_id":"%s"}\n' "$session"
case "$prompt" in *MOCK_FAIL*)
  printf '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"mock failure","session_id":"%s"}\n' "$session"
  exit 1 ;;
esac
reply="mock claude: $(esc "$prompt") [cwd=$(basename "$PWD"), mode=$mode, resumed=$resumed]"
printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"%s"}]},"session_id":"%s"}\n' "$reply" "$session"
printf '{"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn","result":"%s","session_id":"%s","usage":{"input_tokens":3,"output_tokens":7}}\n' "$reply" "$session"
