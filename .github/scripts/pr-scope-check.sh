#!/usr/bin/env bash
# PR scope classifier — first-pass filter, not a perfect classifier.
#
# Usage: pr-scope-check.sh <files.json> <commits.json>
#   files.json    — output of `gh api repos/OWNER/REPO/pulls/<n>/files --paginate`
#                    (array of {filename, status, additions, deletions, patch, previous_filename})
#   commits.json  — output of `gh api repos/OWNER/REPO/pulls/<n>/commits`
#                    (array of {commit: {message}})
#
# Env:
#   ENFORCE               — "true" to actually fail the build on a hard-fail. Default "false"
#                            (informational only: verdict is written, exit code is always 0).
#   GITHUB_STEP_SUMMARY    — path to append the markdown verdict to. Defaults to stdout.
#   PR_SIZE_SOFT_THRESHOLD — net changed lines (additions+deletions, excluding lockfiles) above
#                            which a diff gets soft-flagged as "unusually large". Default 400.
#
# KEEP IN SYNC WITH CONTRIBUTING.md "PR Scope Categories" table — same rules, hand-mirrored.
set -euo pipefail

FILES_JSON="${1:?usage: pr-scope-check.sh <files.json> <commits.json>}"
COMMITS_JSON="${2:?usage: pr-scope-check.sh <files.json> <commits.json>}"
ENFORCE="${ENFORCE:-false}"
SUMMARY="${GITHUB_STEP_SUMMARY:-/dev/stdout}"
SIZE_THRESHOLD="${PR_SIZE_SOFT_THRESHOLD:-700}"

if ! command -v jq >/dev/null 2>&1; then
  echo "pr-scope-check.sh: jq is required" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Load file list
# ---------------------------------------------------------------------------

file_count=$(jq 'length' "$FILES_JSON")
mapfile -t FILENAMES < <(jq -r '.[].filename' "$FILES_JSON")
mapfile -t STATUSES < <(jq -r '.[].status' "$FILES_JSON")
mapfile -t ADDITIONS < <(jq -r '.[].additions' "$FILES_JSON")
mapfile -t DELETIONS < <(jq -r '.[].deletions' "$FILES_JSON")
# previous_filename is only present on renames; jq -r on a missing key gives "null"
mapfile -t PREV_FILENAMES < <(jq -r '.[].previous_filename // ""' "$FILES_JSON")

HARD_FAILS=()
SOFT_FLAGS=()

# ---------------------------------------------------------------------------
# Known top-level allow-list
# ---------------------------------------------------------------------------

ALLOWED_TOP_DIRS=(faderpunk libfp fpapp fpapp-sdk configurator gen-bindings docs .github)
ALLOWED_ROOT_FILES=(
  README.md CONTRIBUTING.md AGENTS.md CLAUDE.md CODE_OF_CONDUCT.md LICENSE
  Cargo.toml Cargo.lock knope.toml devenv.nix devenv.yaml devenv.lock
  build-uf2.sh gen-bindings.sh .editorconfig .envrc .gitignore
)

top_level_segment() {
  # First path segment for a nested path, or the whole name for a root file.
  local path="$1"
  if [[ "$path" == */* ]]; then
    echo "${path%%/*}"
  else
    echo "$path"
  fi
}

path_is_allowed() {
  local path="$1"
  local seg
  seg=$(top_level_segment "$path")
  if [[ "$path" == */* ]]; then
    for d in "${ALLOWED_TOP_DIRS[@]}"; do
      [[ "$seg" == "$d" ]] && return 0
    done
    return 1
  else
    for f in "${ALLOWED_ROOT_FILES[@]}"; do
      [[ "$path" == "$f" ]] && return 0
    done
    return 1
  fi
}

# ---------------------------------------------------------------------------
# Hard-fail 1: unknown top-level path
# ---------------------------------------------------------------------------

UNKNOWN_PATH_FILES=()
for f in "${FILENAMES[@]}"; do
  if ! path_is_allowed "$f"; then
    UNKNOWN_PATH_FILES+=("$f")
  fi
done
if [[ ${#UNKNOWN_PATH_FILES[@]} -gt 0 ]]; then
  HARD_FAILS+=("**Unrecognized top-level path(s)**: touches file(s) outside the known project areas — looks like it introduces an unrelated standalone project. Files: $(printf '`%s`, ' "${UNKNOWN_PATH_FILES[@]}" | sed 's/, $//')")
fi

# ---------------------------------------------------------------------------
# Hard-fail 2: AGENTS.md / CLAUDE.md touched
# ---------------------------------------------------------------------------

for f in "${FILENAMES[@]}"; do
  if [[ "$f" == "AGENTS.md" || "$f" == "CLAUDE.md" ]]; then
    HARD_FAILS+=("**Governance file touched**: \`$f\` — PRs that aren't specifically about agent/contributor policy shouldn't edit this.")
  fi
done

# ---------------------------------------------------------------------------
# Hard-fail 3: CI workflow changes bundled with unrelated changes
# ---------------------------------------------------------------------------

touches_workflow=false
touches_outside_github=false
for f in "${FILENAMES[@]}"; do
  [[ "$f" == .github/workflows/* ]] && touches_workflow=true
  [[ "$f" == .github/* ]] || touches_outside_github=true
done
if [[ "$touches_workflow" == true && "$touches_outside_github" == true ]]; then
  HARD_FAILS+=("**CI/workflow change bundled with unrelated files**: a pure \`.github/workflows/**\` PR is fine on its own, but not mixed with other feature/fix work in the same PR.")
fi

# ---------------------------------------------------------------------------
# Category inference
# ---------------------------------------------------------------------------

is_app_file() { [[ "$1" == faderpunk/src/apps/*.rs && "$1" != faderpunk/src/apps/mod.rs ]]; }
is_manual_file() {
  # docs/apps/*/manual.{md,json} is deliberately NOT accepted here — see
  # faderpunk issue #656: nothing in the configurator actually consumes that
  # format, so a PR that only touches it still needs the missing-manual-entry
  # soft-flag below, not a silent pass.
  case "$1" in
    configurator/src/components/ManualTab.tsx|configurator/src/components/manual/Apps.tsx| \
    configurator/src/components/manual/ManualApp.tsx|configurator/src/components/manual/Md.tsx) return 0 ;;
    *) return 1 ;;
  esac
}

app_files=() app_files_new=() other_apps_touched=()
touches_mod_rs=false touches_manual=false
touches_libfp=false touches_configurator_other=false touches_firmware_core=false
touches_docs=false touches_ci_tooling=false touches_gen_bindings=false
new_app_name=""

for i in "${!FILENAMES[@]}"; do
  f="${FILENAMES[$i]}"
  st="${STATUSES[$i]}"
  if is_app_file "$f"; then
    app_files+=("$f")
    [[ "$st" == "added" ]] && { app_files_new+=("$f"); new_app_name="$(basename "$f" .rs)"; }
  elif [[ "$f" == "faderpunk/src/apps/mod.rs" ]]; then
    touches_mod_rs=true
  elif is_manual_file "$f"; then
    touches_manual=true
  elif [[ "$f" == faderpunk/src/tasks/midi.rs || "$f" == faderpunk/src/storage.rs ]]; then
    : # legitimate common companion touches for app PRs, not counted against any category
  elif [[ "$f" == libfp/src/* || "$f" == "libfp/Cargo.toml" || "$f" == fpapp-sdk/* ]]; then
    touches_libfp=true
  elif [[ "$f" == fpapp/* ]]; then
    touches_ci_tooling=true
  elif [[ "$f" == gen-bindings/* ]]; then
    touches_gen_bindings=true
  elif [[ "$f" == configurator/* ]]; then
    touches_configurator_other=true
  elif [[ "$f" == faderpunk/src/* || "$f" == faderpunk/.cargo/* || "$f" == "faderpunk/memory.x" || "$f" == "faderpunk/build.rs" || "$f" == "faderpunk/Cargo.toml" ]]; then
    # Firmware build config (memory.x, .cargo/config.toml, Cargo.toml) counts as
    # "Firmware core" too — it's allow-listed but otherwise had no category of its own.
    touches_firmware_core=true
  elif [[ "$f" == docs/* || "$f" == "README.md" || "$f" == "CONTRIBUTING.md" || ( "$f" == *.md && "$f" != "AGENTS.md" && "$f" != "CLAUDE.md" ) ]]; then
    touches_docs=true
  elif [[ "$f" == .github/workflows/* || "$f" == ".github/dependabot.yml" || "$f" == "build-uf2.sh" || "$f" == "gen-bindings.sh" || "$f" == devenv.* || "$f" == "knope.toml" ]]; then
    touches_ci_tooling=true
  fi
done

# Any app file belonging to an app other than the one this PR is primarily about
if [[ ${#app_files[@]} -gt 0 ]]; then
  primary_app="$(basename "${app_files[0]}" .rs)"
  for f in "${app_files[@]}"; do
    name="$(basename "$f" .rs)"
    [[ "$name" != "$primary_app" ]] && other_apps_touched+=("$f")
  done
fi

areas_touched=0
[[ ${#app_files[@]} -gt 0 ]] && areas_touched=$((areas_touched + 1))
[[ "$touches_configurator_other" == true ]] && areas_touched=$((areas_touched + 1))
[[ "$touches_libfp" == true ]] && areas_touched=$((areas_touched + 1))
[[ "$touches_firmware_core" == true ]] && areas_touched=$((areas_touched + 1))
[[ "$touches_docs" == true ]] && areas_touched=$((areas_touched + 1))
[[ "$touches_ci_tooling" == true ]] && areas_touched=$((areas_touched + 1))
[[ "$touches_gen_bindings" == true ]] && areas_touched=$((areas_touched + 1))

CATEGORY="Uncategorized/mixed"
if [[ ${#app_files_new[@]} -gt 0 ]]; then
  CATEGORY="App — new ($new_app_name)"
elif [[ ${#app_files[@]} -gt 0 && "$touches_configurator_other" == false && "$touches_firmware_core" == false && "$touches_docs" == false && "$touches_ci_tooling" == false ]]; then
  CATEGORY="App — fix ($(basename "${app_files[0]}" .rs))"
elif [[ ${#app_files[@]} -eq 0 && "$touches_configurator_other" == true && "$touches_libfp" == false && "$touches_firmware_core" == false && "$touches_docs" == false && "$touches_ci_tooling" == false ]]; then
  CATEGORY="Configurator"
elif [[ ${#app_files[@]} -eq 0 && "$touches_configurator_other" == false && ( "$touches_libfp" == true || "$touches_gen_bindings" == true ) && "$touches_firmware_core" == false && "$touches_docs" == false && "$touches_ci_tooling" == false ]]; then
  CATEGORY="Protocol/libfp"
elif [[ ${#app_files[@]} -eq 0 && "$touches_configurator_other" == false && "$touches_libfp" == false && "$touches_firmware_core" == false && "$touches_ci_tooling" == false && "$touches_docs" == true ]]; then
  CATEGORY="Docs"
elif [[ ${#app_files[@]} -eq 0 && "$touches_configurator_other" == false && "$touches_libfp" == false && "$touches_firmware_core" == false && "$touches_docs" == false && "$touches_ci_tooling" == true ]]; then
  CATEGORY="CI/tooling"
elif [[ ${#app_files[@]} -eq 0 && "$touches_configurator_other" == false && "$touches_libfp" == false && "$touches_docs" == false && "$touches_ci_tooling" == false && "$touches_firmware_core" == true ]]; then
  CATEGORY="Firmware core"
fi

is_app_category=false
[[ "$CATEGORY" == "App — new"* || "$CATEGORY" == "App — fix"* ]] && is_app_category=true

if [[ "$CATEGORY" == "Uncategorized/mixed" && $areas_touched -gt 1 ]]; then
  SOFT_FLAGS+=("**Diff spans multiple categories** with no single clear primary area — worth a manual look at what this PR is actually about.")
fi

# ---------------------------------------------------------------------------
# Hard-fail 4: protected files touched by an App-category PR
# ---------------------------------------------------------------------------

if [[ "$is_app_category" == true ]]; then
  for f in "${FILENAMES[@]}"; do
    if [[ "$f" == "configurator/src/components/ManualPage.tsx" || "$f" == "configurator/src/components/Layout.tsx" ]]; then
      HARD_FAILS+=("**Protected file touched by an App PR**: \`$f\` is off-limits for app PRs (never touched by any legitimate sampled app PR).")
    fi
  done
fi

# ---------------------------------------------------------------------------
# Hard-fail 5: safety-API bypass — apps reaching around the App<N> facade
# ---------------------------------------------------------------------------
#
# No exceptions as of 2026-08-21 (PR #635 closed the only two known facade
# gaps — LedMode, AppParams/ParamStore-via-crate::storage — at the source).
# Any of these patterns in an added line is a plain hard-fail.

BANNED_PATTERNS=(
  'crate::storage::'
  'super::storage::'
  'store_global_config' 'load_global_config' 'store_runtime_state' 'load_runtime_state'
  'store_layout' 'load_layout' 'store_calibration_data' 'load_calibration_data'
  'migrate_fram' 'factory_reset'
  'MAX_CHANNEL' 'MaxCmd' 'MaxSender' 'crate::tasks::max' 'super::tasks::max'
  'APP_MIDI_CHANNEL' 'MIDI_CHANNEL' 'MIDI_USB_PUBSUB' 'MIDI_DIN_PUBSUB' 'crate::tasks::midi'
  'I2C_LEADER_PUBLISHER' 'I2C_LEADER_CHANNEL' 'I2C_FOLLOWER_CHANNEL' 'crate::tasks::i2c'
  'BUTTON_PRESSED' 'crate::tasks::buttons'
  'CLOCK_PUBSUB' 'TICK_COUNTER' 'CLOCK_IN_CHANNEL' 'TRANSPORT_CMD_CHANNEL' 'SYNC_ENGINE_CHANNEL' 'crate::tasks::clock'
  'GLOBAL_CONFIG_WATCH' 'set_global_config_via_chan' 'crate::tasks::global_config'
  'EVENT_PUBSUB' 'crate::events'
  'update_state' 'init_state' 'is_clock_running' 'crate::state'
  'crate::tasks::leds'
)

# Extract added lines (strip leading '+', skip the '+++ b/...' file header) from
# a unified diff patch, then flatten multi-line `use crate::{ ... };` groups onto
# a single logical line so a banned module nested inside the block (e.g.
# `storage::{AppParams}` on its own line under an outer `use crate::{`) is still
# matched — a plain per-line grep for "crate::storage::" misses that shape.
extract_added_lines() {
  local patch="$1"
  # Flatten multi-line `use crate::{ ... };` groups onto one logical line
  # (prefixed with a @@BLOCK@@ marker) so the caller can rewrite bare nested
  # module segments (`storage::{...}` under an outer `crate::{`) into
  # crate::-qualified form before matching — see below.
  printf '%s\n' "$patch" | grep -E '^\+' | grep -Ev '^\+\+\+' | sed 's/^\+//' \
    | awk '
        BEGIN { depth = 0; buf = "" }
        {
          line = $0
          if (depth == 0 && line ~ /use[ \t]+crate(::[A-Za-z_0-9]+)*::\{/) {
            depth = 1
            buf = line
            n = gsub(/\{/, "{", line); depth += n - 1
            n2 = gsub(/\}/, "}", line); depth -= n2
            if (depth <= 0) { print "@@BLOCK@@" buf; depth = 0; buf = "" }
            next
          }
          if (depth > 0) {
            buf = buf " " line
            n = gsub(/\{/, "{", line); depth += n
            n2 = gsub(/\}/, "}", line); depth -= n2
            if (depth <= 0) { print "@@BLOCK@@" buf; depth = 0; buf = "" }
            next
          }
          print line
        }
        END { if (depth > 0 && buf != "") print "@@BLOCK@@" buf }
      ' \
    | sed -E '/^@@BLOCK@@/ { s/^@@BLOCK@@//; s/(^|[,{ ])(storage|tasks|events|state)::/\1crate::\2::/g }'
}

app_files_missing_patch=()
for i in "${!FILENAMES[@]}"; do
  f="${FILENAMES[$i]}"
  is_app_file "$f" || continue
  patch=$(jq -r --arg f "$f" '.[] | select(.filename == $f) | .patch // ""' "$FILES_JSON")
  if [[ -z "$patch" ]]; then
    app_files_missing_patch+=("$f")
    continue
  fi
  added="$(extract_added_lines "$patch")"
  file_hits=()
  for pat in "${BANNED_PATTERNS[@]}"; do
    if grep -qF -- "$pat" <<<"$added"; then
      file_hits+=("$pat")
    fi
  done
  if [[ ${#file_hits[@]} -gt 0 ]]; then
    HARD_FAILS+=("**Safety-API bypass** in \`$f\`: reaches a Core-0 hardware-task internal directly instead of through the \`App<N>\` facade (\`use crate::app::{...}\`). Matched: $(printf '`%s`, ' "${file_hits[@]}" | sed 's/, $//')")
  fi
done
if [[ ${#app_files_missing_patch[@]} -gt 0 ]]; then
  for f in "${app_files_missing_patch[@]}"; do
    SOFT_FLAGS+=("Couldn't statically verify safety-API compliance for \`$f\` (diff too large for GitHub to return a patch) — needs manual review.")
  done
fi

# ---------------------------------------------------------------------------
# Soft-flags
# ---------------------------------------------------------------------------

if [[ "$is_app_category" == true && "$touches_libfp" == true ]]; then
  SOFT_FLAGS+=("App PR also touches \`libfp/src/**\` — legitimate before (e.g. GenSeq's shared slide utility), but worth a look.")
fi

if [[ ${#other_apps_touched[@]} -gt 0 ]]; then
  SOFT_FLAGS+=("Touches other apps' files besides its own: $(printf '`%s`, ' "${other_apps_touched[@]}" | sed 's/, $//')")
fi

if [[ ${#app_files_new[@]} -gt 0 ]]; then
  [[ "$touches_mod_rs" == false ]] && SOFT_FLAGS+=("New app file added but \`faderpunk/src/apps/mod.rs\` isn't touched — app won't be registered.")
  [[ "$touches_manual" == false ]] && SOFT_FLAGS+=("New app file added but no manual entry found in \`ManualTab.tsx\`/\`manual/Apps.tsx\` — this is the only format the configurator actually renders (see issue #656).")
fi

total_net=0
for i in "${!FILENAMES[@]}"; do
  f="${FILENAMES[$i]}"
  [[ "$f" == "Cargo.lock" || "$f" == "*/Cargo.lock" || "$f" == *"Cargo.lock" || "$f" == *"pnpm-lock.yaml" ]] && continue
  total_net=$((total_net + ADDITIONS[i] + DELETIONS[i]))
done
if [[ $total_net -gt $SIZE_THRESHOLD ]]; then
  SOFT_FLAGS+=("Large diff: $total_net net changed lines (excluding lockfiles), over the ${SIZE_THRESHOLD}-line soft threshold.")
fi

# fix/feat commit-type mixing
mapfile -t COMMIT_MSGS < <(jq -r '.[].commit.message // "" | split("\n")[0]' "$COMMITS_JSON")
has_fix=false has_feat=false
for m in "${COMMIT_MSGS[@]}"; do
  [[ "$m" =~ ^fix(\(.*\))?!?: ]] && has_fix=true
  [[ "$m" =~ ^feat(\(.*\))?!?: ]] && has_feat=true
done
if [[ "$has_fix" == true && "$has_feat" == true ]]; then
  SOFT_FLAGS+=("Commits mix \`fix:\` and \`feat:\` types — this PR appears to bundle a bug fix and a new feature. Consider splitting into two PRs.")
fi

# renamed files: sanity note only (status/previous_filename already accounted for
# via jq above; renamed files are otherwise treated the same as their new path)

# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------

{
  echo "## PR Scope Check"
  echo
  echo "**Category**: $CATEGORY"
  echo "**Files changed**: $file_count"
  echo "**Enforcement**: $([ "$ENFORCE" == "true" ] && echo "ON — hard-fails below block this check" || echo "OFF (informational only) — nothing here blocks merging yet")"
  echo
  if [[ ${#HARD_FAILS[@]} -gt 0 ]]; then
    echo "### ❌ Hard-fail$([ ${#HARD_FAILS[@]} -gt 1 ] && echo s) (${#HARD_FAILS[@]})"
    echo
    for h in "${HARD_FAILS[@]}"; do echo "- $h"; done
    echo
  else
    echo "### ✅ No hard-fails"
    echo
  fi
  if [[ ${#SOFT_FLAGS[@]} -gt 0 ]]; then
    echo "### ⚠️ Needs a human look (${#SOFT_FLAGS[@]})"
    echo
    for s in "${SOFT_FLAGS[@]}"; do echo "- $s"; done
    echo
  fi
  echo "### Changed files"
  echo
  for f in "${FILENAMES[@]}"; do echo "- \`$f\`"; done
} >> "$SUMMARY"

if [[ "$ENFORCE" == "true" && ${#HARD_FAILS[@]} -gt 0 ]]; then
  exit 1
fi
exit 0
