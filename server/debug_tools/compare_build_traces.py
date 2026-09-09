#!/usr/bin/env python3
"""Compare two build-trace JSON dumps produced by `BuildTracer`
(see server/src/core/build_trace.rs, gated by DEBUG_DUMP_BUILD_TRACE).

Each trace is a flat, chronologically ordered list of events:
    {"event": "queued", "order": N, "step": "ARCH", "kind": "File",
     "symbol": "...", "triggered_by": "... or null"}
    {"event": "built",  "order": N, "step": "ARCH", "kind": "File",
     "symbol": "..."}

This tool diffs two such traces (typically one per commit/branch) to answer:
  - which symbols got built in one run but not the other ("added"/"removed")
  - whether the relative build order drifted, and by how much ("reordered")
  - whether the dependency that triggered a symbol's queuing changed
    ("triggered-by")
  - whether a symbol moved to a different build step ("step-changed")
  - which symbols get (re)built more than once, and whether that build count
    changed between the two runs ("counts")

By default, ODOO_FUNCTION_AE is treated as the same step as ARCH_EVAL (it is
the old ARCH_EVAL step split in two - see --merge-step / --no-merge-steps to
change that): a symbol built via ARCH_EVAL alone on one side and via
ARCH_EVAL + ODOO_FUNCTION_AE on the other is considered unchanged, not
flagged as added/removed/reordered/step-changed.

Usage examples:
    # Orientation: counts only.
    python3 compare_build_traces.py --a before.json --b after.json

    # What got built in `after` that wasn't built in `before`, Files only.
    python3 compare_build_traces.py --a before.json --b after.json \\
        --mode added --event built --kind File

    # Biggest order-drift within the ARCH_EVAL step.
    python3 compare_build_traces.py --a before.json --b after.json \\
        --mode reordered --step ARCH_EVAL --limit 30

    # Symbols built more than once, or whose build count changed.
    python3 compare_build_traces.py --a before.json --b after.json \\
        --mode counts --kind File

    # Everything, dumped as JSON for further processing.
    python3 compare_build_traces.py --a before.json --b after.json \\
        --mode all --format json --out report.json

No third-party dependencies; stdlib only.
"""
import argparse
import json
import re
import sys
from bisect import bisect_left
from collections import defaultdict


# ODOO_FUNCTION_AE is the old ARCH_EVAL step split in two (see the "add
# function arch step" commit): by default we treat it as ARCH_EVAL so the
# split itself doesn't show up as noise in the diff. Override with
# --merge-step / disable with --no-merge-steps.
DEFAULT_STEP_MERGES = {"ODOO_FUNCTION_AE": "ARCH_EVAL"}


# ---------------------------------------------------------------------------
# Loading & filtering
# ---------------------------------------------------------------------------

def load_trace(path):
    with open(path, "r") as f:
        return json.load(f)


def apply_step_merges(events, step_map):
    """Normalize each event's `step` in place per `step_map`, stashing the
    original under `_raw_step` (used by `disambiguate` to tell a genuine
    repeat build apart from two different raw steps merged into one)."""
    for e in events:
        raw = e["step"]
        e["_raw_step"] = raw
        e["step"] = step_map.get(raw, raw)


def filter_events(events, event_type, steps, kinds, symbol_re):
    steps = set(steps) if steps else None
    kinds = set(kinds) if kinds else None
    pattern = re.compile(symbol_re) if symbol_re else None
    out = []
    for e in events:
        if event_type and e["event"] != event_type:
            continue
        if steps and e["step"] not in steps:
            continue
        if kinds and e["kind"] not in kinds:
            continue
        if pattern and not pattern.search(e["symbol"]):
            continue
        out.append(e)
    return out


def disambiguate(events):
    """Assign each event a key unique within its own sequence:
    (kind, step, symbol, occurrence). `step` here is the (possibly merged,
    see apply_step_merges) normalized step.

    Two events land in the same occurrence bucket - i.e. are treated as one
    logical build - as long as they come from *different* raw steps that got
    merged into the same normalized step (e.g. one ARCH_EVAL + one
    ODOO_FUNCTION_AE event for the same symbol). A bucket only advances to
    the next occurrence once a raw step repeats within it, which is the
    signature of a genuine second build rather than a merge artifact.

    Repeated symbols (e.g. two functions named the same in different
    classes/files - `Function` events only carry a bare name, see
    BuildTracer's caveat) pair up in the order they occur, which is the same
    heuristic textual diff tools use for repeated lines.

    Returns list of (key, event) in original order. Note: distinct events
    can map to the *same* key (that's the merge collapsing them into one
    logical entry) - downstream dict-building must resolve that explicitly
    (first occurrence wins, see `_first_wins`) rather than assume uniqueness.
    """
    bucket_state = {}
    keyed = []
    for e in events:
        gkey = (e["kind"], e["step"], e["symbol"])
        occ, raw_seen = bucket_state.get(gkey, (0, frozenset()))
        raw = e.get("_raw_step", e["step"])
        if raw in raw_seen:
            occ += 1
            raw_seen = frozenset()
        raw_seen = raw_seen | {raw}
        bucket_state[gkey] = (occ, raw_seen)
        keyed.append(((*gkey, occ), e))
    return keyed


def _first_wins(keyed):
    """Build a key -> event dict from a `disambiguate` result, keeping the
    first event seen for each key (keys can repeat, see `disambiguate`)."""
    out = {}
    for k, e in keyed:
        if k not in out:
            out[k] = e
    return out


# ---------------------------------------------------------------------------
# Order-drift: longest increasing subsequence (patience-sort), O(n log n)
# ---------------------------------------------------------------------------

def lis_indices(seq):
    """Indices (into `seq`) forming one longest strictly-increasing
    subsequence of values. Standard patience-sorting LIS."""
    tails = []       # tails[k] = index into seq of smallest tail value of an
                      # increasing run of length k+1
    prev = [-1] * len(seq)
    for i, x in enumerate(seq):
        lo, hi = 0, len(tails)
        while lo < hi:
            mid = (lo + hi) // 2
            if seq[tails[mid]] < x:
                lo = mid + 1
            else:
                hi = mid
        if lo > 0:
            prev[i] = tails[lo - 1]
        if lo == len(tails):
            tails.append(i)
        else:
            tails[lo] = i
    result = set()
    k = tails[-1] if tails else -1
    while k != -1:
        result.add(k)
        k = prev[k]
    return result


# ---------------------------------------------------------------------------
# Report builders
# ---------------------------------------------------------------------------

def build_added_removed(keyed_a, keyed_b):
    keys_a = {k for k, _ in keyed_a}
    keys_b = {k for k, _ in keyed_b}
    added = keys_b - keys_a
    removed = keys_a - keys_b
    ev_by_key_a = _first_wins(keyed_a)
    ev_by_key_b = _first_wins(keyed_b)
    added_events = [ev_by_key_b[k] for k in sorted(added, key=lambda k: ev_by_key_b[k]["order"])]
    removed_events = [ev_by_key_a[k] for k in sorted(removed, key=lambda k: ev_by_key_a[k]["order"])]
    return added_events, removed_events


def build_reordered(keyed_a, keyed_b, min_shift):
    index_a = {}
    for i, (k, _) in enumerate(keyed_a):
        index_a.setdefault(k, i)
    index_b = {}
    for i, (k, _) in enumerate(keyed_b):
        index_b.setdefault(k, i)
    common = [k for k in index_a if k in index_b]
    common.sort(key=lambda k: index_a[k])
    n_a, n_b = len(keyed_a), len(keyed_b)
    if not common:
        return []
    b_positions = [index_b[k] for k in common]
    stable = lis_indices(b_positions)
    reordered = []
    for pos, k in enumerate(common):
        if pos in stable:
            continue
        ia, ib = index_a[k], index_b[k]
        shift = ib / max(n_b - 1, 1) - ia / max(n_a - 1, 1)
        if abs(shift) < min_shift:
            continue
        kind, step, symbol, occ = k
        reordered.append({
            "kind": kind, "step": step, "symbol": symbol, "occurrence": occ,
            "index_a": ia, "index_b": ib, "normalized_shift": shift,
        })
    reordered.sort(key=lambda r: -abs(r["normalized_shift"]))
    return reordered


def build_triggered_by_changes(keyed_a_queued, keyed_b_queued):
    by_key_a = _first_wins(keyed_a_queued)
    by_key_b = _first_wins(keyed_b_queued)
    changes = []
    for k in by_key_a:
        if k not in by_key_b:
            continue
        ta = by_key_a[k].get("triggered_by")
        tb = by_key_b[k].get("triggered_by")
        if ta != tb:
            kind, step, symbol, occ = k
            changes.append({
                "kind": kind, "step": step, "symbol": symbol, "occurrence": occ,
                "triggered_by_a": ta, "triggered_by_b": tb,
            })
    changes.sort(key=lambda c: (c["kind"], c["symbol"]))
    return changes


def build_step_changes(built_a, built_b):
    """Symbols (ignoring occurrence) whose *set* of (normalized) build steps
    differs between the two runs - e.g. a symbol validated directly in one
    run but routed through an extra step in the other. Steps merged via
    apply_step_merges (ARCH_EVAL/ODOO_FUNCTION_AE by default) collapse to
    the same set entry and so never show up here on their own."""
    steps_a = defaultdict(set)
    steps_b = defaultdict(set)
    for e in built_a:
        steps_a[(e["kind"], e["symbol"])].add(e["step"])
    for e in built_b:
        steps_b[(e["kind"], e["symbol"])].add(e["step"])
    keys = set(steps_a) | set(steps_b)
    changes = []
    for k in keys:
        sa, sb = steps_a.get(k, set()), steps_b.get(k, set())
        if sa != sb:
            kind, symbol = k
            changes.append({
                "kind": kind, "symbol": symbol,
                "steps_a": sorted(sa), "steps_b": sorted(sb),
            })
    changes.sort(key=lambda c: (c["kind"], c["symbol"]))
    return changes


def build_counts(events_a, events_b):
    """How many times each (kind, symbol, step) was built/queued in each
    trace. Returns (duplicates, count_changed):
      - duplicates: rows built/queued more than once in *either* trace
        (count_a and count_b both shown - a symbol can be a legitimate,
        consistent duplicate in both without its count having changed).
      - count_changed: rows where the count differs between the two traces.
    A row can appear in both lists. `step` is the normalized step, so a
    symbol built once via ARCH_EVAL and once via ODOO_FUNCTION_AE on one
    side (the merged split, see apply_step_merges) counts as 1, not 2.
    """
    def counter(events):
        c = defaultdict(int)
        for e in events:
            c[(e["kind"], e["symbol"], e["step"])] += 1
        return c
    ca, cb = counter(events_a), counter(events_b)
    duplicates, changed = [], []
    for k in set(ca) | set(cb):
        na, nb = ca.get(k, 0), cb.get(k, 0)
        kind, symbol, step = k
        row = {"kind": kind, "symbol": symbol, "step": step, "count_a": na, "count_b": nb, "delta": nb - na}
        if na > 1 or nb > 1:
            duplicates.append(row)
        if na != nb:
            changed.append(row)
    duplicates.sort(key=lambda r: -max(r["count_a"], r["count_b"]))
    changed.sort(key=lambda r: -abs(r["delta"]))
    return duplicates, changed


def build_summary(events_a, events_b, label_a, label_b):
    def counts(events):
        by_event_step = defaultdict(int)
        for e in events:
            by_event_step[(e["event"], e["step"])] += 1
        return dict(sorted(by_event_step.items()))
    return {
        label_a: {"total": len(events_a), "by_event_step": {f"{k[0]}/{k[1]}": v for k, v in counts(events_a).items()}},
        label_b: {"total": len(events_b), "by_event_step": {f"{k[0]}/{k[1]}": v for k, v in counts(events_b).items()}},
    }


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------

def print_human(mode, report, limit):
    def head(title):
        print(f"\n=== {title} ===")

    if "summary" in report:
        head("Summary")
        for label, data in report["summary"].items():
            print(f"{label}: {data['total']} events")
            for k, v in data["by_event_step"].items():
                print(f"    {k}: {v}")

    if "added" in report:
        head(f"Added in B, not in A ({len(report['added'])})")
        for e in report["added"][:limit]:
            print(f"  [{e['step']:>16}] {e['kind']:<14} {e['symbol']}")
        if len(report["added"]) > limit:
            print(f"  ... {len(report['added']) - limit} more (raise --limit)")

    if "removed" in report:
        head(f"Removed in B (present in A only) ({len(report['removed'])})")
        for e in report["removed"][:limit]:
            print(f"  [{e['step']:>16}] {e['kind']:<14} {e['symbol']}")
        if len(report["removed"]) > limit:
            print(f"  ... {len(report['removed']) - limit} more (raise --limit)")

    if "reordered" in report:
        head(f"Reordered ({len(report['reordered'])} shown, sorted by magnitude)")
        for r in report["reordered"][:limit]:
            print(f"  {r['normalized_shift']:+.4f}  [{r['step']:>16}] {r['kind']:<14} "
                  f"{r['symbol']}  (A#{r['index_a']} -> B#{r['index_b']})")
        if len(report["reordered"]) > limit:
            print(f"  ... {len(report['reordered']) - limit} more (raise --limit)")

    if "triggered_by" in report:
        head(f"Triggered-by changed ({len(report['triggered_by'])})")
        for c in report["triggered_by"][:limit]:
            print(f"  [{c['step']:>16}] {c['kind']:<14} {c['symbol']}")
            print(f"      A: {c['triggered_by_a']}")
            print(f"      B: {c['triggered_by_b']}")
        if len(report["triggered_by"]) > limit:
            print(f"  ... {len(report['triggered_by']) - limit} more (raise --limit)")

    if "step_changed" in report:
        head(f"Build-step set changed ({len(report['step_changed'])})")
        for c in report["step_changed"][:limit]:
            print(f"  {c['kind']:<14} {c['symbol']}")
            print(f"      A: {c['steps_a']}")
            print(f"      B: {c['steps_b']}")
        if len(report["step_changed"]) > limit:
            print(f"  ... {len(report['step_changed']) - limit} more (raise --limit)")

    if "duplicates" in report:
        head(f"Built/queued more than once in A or B ({len(report['duplicates'])})")
        for r in report["duplicates"][:limit]:
            print(f"  [{r['step']:>16}] {r['kind']:<14} {r['symbol']}  A={r['count_a']} B={r['count_b']}")
        if len(report["duplicates"]) > limit:
            print(f"  ... {len(report['duplicates']) - limit} more (raise --limit)")

    if "count_changed" in report:
        head(f"Build/queue count changed between A and B ({len(report['count_changed'])})")
        for r in report["count_changed"][:limit]:
            print(f"  {r['delta']:+3d}  [{r['step']:>16}] {r['kind']:<14} {r['symbol']}  A={r['count_a']} B={r['count_b']}")
        if len(report["count_changed"]) > limit:
            print(f"  ... {len(report['count_changed']) - limit} more (raise --limit)")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--a", required=True, help="First trace file (e.g. previous commit / 'before')")
    parser.add_argument("--b", required=True, help="Second trace file (e.g. current commit / 'after')")
    parser.add_argument("--name-a", default="A", help="Label for --a in output")
    parser.add_argument("--name-b", default="B", help="Label for --b in output")
    parser.add_argument("--mode", choices=["summary", "added", "removed", "reordered", "triggered-by", "step-changed", "counts", "all"],
                         default="summary", help="What to report (default: summary)")
    parser.add_argument("--event", choices=["queued", "built"], default="built",
                         help="Which event type to compare for added/removed/reordered/counts (default: built)")
    parser.add_argument("--step", action="append", default=[],
                         help="Restrict to this (normalized, i.e. post-merge) BuildSteps value (repeatable): "
                              "ARCH, ARCH_EVAL, VALIDATION")
    parser.add_argument("--kind", action="append", default=[],
                         help="Restrict to this symbol kind (repeatable): File, Module, PythonPackage, Function, XmlFile, CsvFile, JsFile")
    parser.add_argument("--symbol-regex", default=None, help="Only keep symbols matching this regex")
    parser.add_argument("--merge-step", action="append", default=[], metavar="OLD=NEW",
                         help="Treat step OLD as step NEW for comparison purposes (repeatable). "
                              f"Default: {', '.join(f'{k}={v}' for k, v in DEFAULT_STEP_MERGES.items())}")
    parser.add_argument("--no-merge-steps", action="store_true",
                         help="Disable the default step merges above (compare raw BuildSteps values as-is)")
    parser.add_argument("--min-shift", type=float, default=0.0,
                         help="For --mode reordered: minimum |normalized order shift| (0..1) to report (default: 0, i.e. any drift)")
    parser.add_argument("--limit", type=int, default=50, help="Max rows printed per section in human output (default: 50)")
    parser.add_argument("--format", choices=["human", "json"], default="human")
    parser.add_argument("--out", default=None, help="Write output to this file instead of stdout")
    args = parser.parse_args()

    step_map = {} if args.no_merge_steps else dict(DEFAULT_STEP_MERGES)
    for spec in args.merge_step:
        old, sep, new = spec.partition("=")
        if not sep:
            parser.error(f"--merge-step expects OLD=NEW, got {spec!r}")
        step_map[old] = new

    events_a = load_trace(args.a)
    events_b = load_trace(args.b)
    apply_step_merges(events_a, step_map)
    apply_step_merges(events_b, step_map)

    report = {}

    if args.mode in ("summary", "all"):
        report["summary"] = build_summary(events_a, events_b, args.name_a, args.name_b)

    if args.mode in ("added", "removed", "reordered", "all"):
        filt_a = filter_events(events_a, args.event, args.step, args.kind, args.symbol_regex)
        filt_b = filter_events(events_b, args.event, args.step, args.kind, args.symbol_regex)
        keyed_a = disambiguate(filt_a)
        keyed_b = disambiguate(filt_b)

        if args.mode in ("added", "removed", "all"):
            added, removed = build_added_removed(keyed_a, keyed_b)
            if args.mode in ("added", "all"):
                report["added"] = added
            if args.mode in ("removed", "all"):
                report["removed"] = removed

        if args.mode in ("reordered", "all"):
            report["reordered"] = build_reordered(keyed_a, keyed_b, args.min_shift)

    if args.mode in ("triggered-by", "all"):
        filt_a = filter_events(events_a, "queued", args.step, args.kind, args.symbol_regex)
        filt_b = filter_events(events_b, "queued", args.step, args.kind, args.symbol_regex)
        report["triggered_by"] = build_triggered_by_changes(disambiguate(filt_a), disambiguate(filt_b))

    if args.mode in ("step-changed", "all"):
        filt_a = filter_events(events_a, "built", None, args.kind, args.symbol_regex)
        filt_b = filter_events(events_b, "built", None, args.kind, args.symbol_regex)
        report["step_changed"] = build_step_changes(filt_a, filt_b)

    if args.mode in ("counts", "all"):
        filt_a = filter_events(events_a, args.event, args.step, args.kind, args.symbol_regex)
        filt_b = filter_events(events_b, args.event, args.step, args.kind, args.symbol_regex)
        report["duplicates"], report["count_changed"] = build_counts(filt_a, filt_b)

    out = open(args.out, "w") if args.out else sys.stdout
    try:
        if args.format == "json":
            json.dump(report, out, indent=2)
            out.write("\n")
        else:
            print_human(args.mode, report, args.limit)
    finally:
        if args.out:
            out.close()


if __name__ == "__main__":
    main()
