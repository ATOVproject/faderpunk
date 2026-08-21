#!/usr/bin/env bash
# Regression test for pr-scope-check.sh against frozen fixtures.
#
# Fixtures in testdata/ are real `gh api pulls/<n>/files` + `pulls/<n>/commits`
# snapshots (named `<pr-number>-*.json`) captured 2026-08-21, plus a few
# hand-crafted synthetic edge cases (named `synthetic-*.json`). Real-PR
# fixtures are frozen snapshots — they don't depend on those PRs staying open
# or unchanged on GitHub.
#
# Usage: .github/scripts/pr-scope-check.test.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK="$SCRIPT_DIR/pr-scope-check.sh"
DATA="$SCRIPT_DIR/testdata"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

pass=0
fail=0

# name : expect_exit : expect_hard_fail_count (or "any">=1 style via -ge marker) : note
# Format per line: "<fixture> <expected_exit> <expected_hardfail_op><n>"
#   expected_hardfail_op is "=" or ">="
CASES=(
  "573 0 =0"
  "602 1 >=1"
  "612 1 >=1"
  "529 0 =0"
  "629 1 =2"
  "521 1 =1"
  "607 1 =2"
  "603 1 =1"
  "474 1 =1"
  "637 1 =1"
  "645 0 =0"
  "601 0 =0"
  "614 0 =0"
  "synthetic-mixed-commits 0 =0"
  "synthetic-nested-use-bypass 1 =1"
  "synthetic-rename 0 =0"
)

for case_line in "${CASES[@]}"; do
  read -r name expect_exit hf_spec <<<"$case_line"
  files="$DATA/$name-files.json"
  commits="$DATA/$name-commits.json"
  if [[ ! -f "$files" || ! -f "$commits" ]]; then
    echo "SKIP $name (fixture missing: $files / $commits)"
    continue
  fi

  summary="$TMP/$name-summary.md"
  set +e
  GITHUB_STEP_SUMMARY="$summary" ENFORCE=true bash "$CHECK" "$files" "$commits" >/dev/null 2>"$TMP/$name.err"
  actual_exit=$?
  set -e

  hf_count=$(grep -c '^- \*\*' "$summary" 2>/dev/null || true)
  # Hard-fail bullets appear between "Hard-fail" heading and the next "###";
  # count bullet lines that start the hard-fail markdown convention used by
  # the script ("- **<Title>**...").  Soft-flags also use "- " but not "**"
  # immediately, except a few — so instead, count precisely between markers.
  hf_count=$(awk '/^### ❌/{f=1;next}/^### /{f=0}f && /^- /{c++}END{print c+0}' "$summary")

  op="${hf_spec:0:1}"
  if [[ "$op" == ">" ]]; then
    expect_n="${hf_spec:2}"
    hf_ok=$([[ $hf_count -ge $expect_n ]] && echo yes || echo no)
    hf_desc=">=$expect_n"
  else
    expect_n="${hf_spec:1}"
    hf_ok=$([[ $hf_count -eq $expect_n ]] && echo yes || echo no)
    hf_desc="=$expect_n"
  fi

  status="ok"
  [[ "$actual_exit" -ne "$expect_exit" ]] && status="FAIL(exit: got $actual_exit want $expect_exit)"
  [[ "$hf_ok" == "no" ]] && status="$status FAIL(hardfails: got $hf_count want $hf_desc)"

  if [[ "$status" == "ok" ]]; then
    echo "PASS  $name  (exit=$actual_exit, hard-fails=$hf_count)"
    pass=$((pass + 1))
  else
    echo "FAIL  $name  $status"
    fail=$((fail + 1))
  fi
done

echo
echo "$pass passed, $fail failed"
[[ $fail -eq 0 ]]
