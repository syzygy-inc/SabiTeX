#!/usr/bin/env sh
# 台帳（target/qa-ledger/*.tsv）を specification/cases.md の予定 case と突き合わせる。
# 使い方: rm -rf target/qa-ledger; SABI_STRICT_TESTS=1 cargo test --workspace; scripts/qa-ledger.sh
# 予定した必須 case がすべて PASS でなければ 1 で終わる。
set -eu
cd "$(dirname "$0")/.."
ledger_dir="${SABI_QA_LEDGER:-target/qa-ledger}"
cases=$(grep -E '^\| [A-Z]+-[A-Z0-9-]+ \|' specification/cases.md | awk -F'|' '{gsub(/ /,"",$2); gsub(/ /,"",$5); print $2, $5}')
status=0
planned=0
passed=0
printf '%-22s %-9s %-9s %s\n' case expected status comparisons
for line in $(echo "$cases" | tr ' ' ':'); do
  id=${line%%:*}
  need=${line#*:}
  planned=$((planned + 1))
  rec=$(cat "$ledger_dir"/*.tsv 2>/dev/null | awk -F'\t' -v id="$id" '$1 == id { last = $0 } END { print last }')
  if [ -z "$rec" ]; then
    st="NOT-RUN"; cmp="-"
  else
    st=$(printf '%s' "$rec" | cut -f2)
    cmp=$(printf '%s' "$rec" | cut -f3)
  fi
  printf '%-22s %-9s %-9s %s\n' "$id" "$need" "$st" "$cmp"
  if [ "$st" = "PASS" ]; then
    passed=$((passed + 1))
  elif [ "$need" = "required" ]; then
    status=1
  fi
done
echo "planned $planned, passed $passed"
exit $status
