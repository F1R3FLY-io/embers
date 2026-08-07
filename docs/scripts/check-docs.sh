#!/usr/bin/env bash
#
# Documentation checks (wired as `cargo make docs-check`):
#   1. every relative Markdown link in docs/ + the crate READMEs + the root README
#      resolves to an existing file;
#   2. every docs/diagrams/*.puml has a committed *.svg sibling;
#   3. re-rendering each *.puml reproduces the committed *.svg byte-for-byte and is
#      not an error image (PlantUML SVG output is byte-reproducible), catching both
#      broken diagrams and diagrams whose source changed without a re-render.
#
# Exits non-zero on any failure so it can gate CI.

set -uo pipefail
cd "$(dirname "$0")/../.." || exit 2   # repo root

status=0

echo "== 1. internal Markdown links =="
python3 - <<'PY' || status=1
import os, re, glob, sys
mdfiles = sorted(
    set(glob.glob('docs/**/*.md', recursive=True))
    | set(glob.glob('packages/**/README.md', recursive=True))
    | {'README.md'}
)
link_re = re.compile(r'\[[^\]]+\]\(([^)]+)\)')
dangling = 0
for md in mdfiles:
    base = os.path.dirname(md)
    for m in link_re.finditer(open(md, encoding='utf-8').read()):
        tgt = m.group(1).split('#')[0].strip()
        if not tgt or tgt.startswith(('http://', 'https://', 'mailto:')):
            continue
        if not os.path.exists(os.path.normpath(os.path.join(base, tgt))):
            print(f'  DANGLING: {md} -> {tgt}')
            dangling += 1
if dangling:
    print(f'  {dangling} dangling link(s)')
    sys.exit(1)
print('  ok: all internal links resolve')
PY

echo "== 2/3. diagrams (svg present, reproducible, not error images) =="
if command -v plantuml >/dev/null 2>&1; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  shopt -s nullglob
  for puml in docs/diagrams/*.puml; do
    svg="${puml%.puml}.svg"
    if [[ ! -f "$svg" ]]; then
      echo "  MISSING SVG: $svg"; status=1; continue
    fi
    cp "$puml" "$tmp/"
    plantuml -tsvg "$tmp/$(basename "$puml")" >/dev/null 2>&1
    rendered="$tmp/$(basename "$svg")"
    if [[ ! -f "$rendered" ]] || grep -qi 'contains errors\|syntax error' "$rendered"; then
      echo "  RENDER ERROR: $puml"; status=1; continue
    fi
    if ! diff -q "$rendered" "$svg" >/dev/null 2>&1; then
      echo "  STALE SVG (re-render differs — run plantuml -tsvg): $svg"; status=1
    fi
  done
  [[ $status -eq 0 ]] && echo "  ok: all diagrams render and match"
else
  echo "  warn: plantuml not on PATH — skipping diagram checks"
fi

if [[ $status -eq 0 ]]; then
  echo "docs-check: PASS"
else
  echo "docs-check: FAIL"
fi
exit $status
