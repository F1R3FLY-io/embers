#!/usr/bin/env python3
"""Explicit-state checker for the embers lifecycle FSMs (formal target T10).

Reads every *.lts.json under this directory and verifies its declared properties by
explicit-state model checking — a small, dependency-free, CI-independent re-check of
the same FSMs that the pgmcp `behavioral_check` tool checks and that the A5 lifecycle
property tests mirror. Exits non-zero if any property fails.

Supported property kinds (each is a structural or CTL-reachability check):
  reachable(state)                    EF from the initial state reaches `state`.
  absorbing(states)                   each listed state's only outgoing edge is a self-loop.
  no_dead_states                      every reachable state has an outgoing edge.
  from_only(label, allowed)           every edge with `label` starts in `allowed`.
  into_only_from(target, allowed)     every edge into `target` starts in `allowed`.
  reach_from(source, targets)         AG-style: from `source`, EF reaches some `targets`.
"""
import glob
import json
import os
import sys


def reachable_from(adj, start):
    seen, stack = {start}, [start]
    while stack:
        for to, _ in adj.get(stack.pop(), []):
            if to not in seen:
                seen.add(to)
                stack.append(to)
    return seen


def can_reach(adj, sources, targets):
    """Backward reachability: states from which some target is reachable (EF targets)."""
    rev = {}
    for frm, edges in adj.items():
        for to, _ in edges:
            rev.setdefault(to, []).append(frm)
    seen, stack = set(targets), list(targets)
    while stack:
        for pre in rev.get(stack.pop(), []):
            if pre not in seen:
                seen.add(pre)
                stack.append(pre)
    return {s for s in sources if s in seen}


def check_model(path):
    m = json.load(open(path, encoding="utf-8"))
    name = m["name"]
    states = set(m["states"])
    initial = m["initial"]
    adj = {s: [] for s in states}
    for t in m["transitions"]:
        assert t["from"] in states, f"{name}: unknown from-state {t['from']}"
        assert t["to"] in states, f"{name}: unknown to-state {t['to']}"
        adj[t["from"]].append((t["to"], t.get("label", "")))
    reach = reachable_from(adj, initial)

    failures = []
    for p in m["properties"]:
        kind = p["kind"]
        ok = True
        if kind == "reachable":
            ok = p["state"] in reach
        elif kind == "absorbing":
            for s in p["states"]:
                outs = adj.get(s, [])
                if not outs or any(to != s for to, _ in outs):
                    ok = False
        elif kind == "no_dead_states":
            ok = all(adj.get(s) for s in reach)
        elif kind == "from_only":
            srcs = {frm for frm, edges in adj.items() for _, lbl in edges if lbl == p["label"]}
            ok = srcs.issubset(set(p["allowed"]))
        elif kind == "into_only_from":
            srcs = {frm for frm, edges in adj.items() for to, _ in edges if to == p["target"]}
            # a self-loop on the target does not count as an external entry
            srcs.discard(p["target"])
            ok = srcs.issubset(set(p["allowed"]))
        elif kind == "reach_from":
            reachers = can_reach(adj, [p["source"]], p["targets"])
            ok = p["source"] in reachers
        else:
            failures.append(f"  {name}: unknown property kind {kind!r}")
            continue
        status = "ok  " if ok else "FAIL"
        print(f"  [{status}] {name}: {p['id']}")
        if not ok:
            failures.append(f"  {name}: {p['id']} ({kind})")
    return failures


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    models = sorted(glob.glob(os.path.join(here, "*.lts.json")))
    if not models:
        print("no *.lts.json models found", file=sys.stderr)
        return 2
    all_failures = []
    for path in models:
        all_failures += check_model(path)
    if all_failures:
        print("\nlifecycle check FAILED:")
        print("\n".join(all_failures))
        return 1
    print("\nall lifecycle properties hold")
    return 0


if __name__ == "__main__":
    sys.exit(main())
