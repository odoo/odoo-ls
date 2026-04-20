"""
Auto-deref pretty-printer for specific Rust reference types whose pointee
has a working pretty-printer but whose reference form does not.

rust-gdb's bundled printers match referent types exactly
(e.g. `alloc::vec::Vec<T>`), so `&Vec<T>` falls back to the raw
`RawVec { ptr, cap } / len` view.

This script adds a narrow pretty-printer that recognizes a curated list of
referent types behind a Rust reference and delegates to their default
visualizer. Both `TYPE_CODE_REF` and `TYPE_CODE_PTR` are considered
(rustc emits `&T` as one or the other depending on toolchain version);
what keeps this safe is the strict referent-name whitelist — we only
fire on types that idiomatic Rust never exposes as raw pointers, so
`RawVec.ptr`, `NonNull<T>`, etc. are left alone.

Currently targets only `&Vec<T>`. Other references (e.g. `&WeakSet`) render
correctly on their own and are intentionally not touched. Extend
`_AUTO_DEREF_PATTERNS` if a new case shows up.

Load via ~/.gdbinit or a .vscode/launch.json setupCommand:
    source /path/to/rust_ref_printers.py

Requires rust-gdb — the delegated visualizers are rustc's Python printers
registered through `gdb.default_visualizer`.
"""

import re

import gdb

# Referent type-name patterns (matched against the canonical stringified
# type after strip_typedefs). Keep this list tight — every reference in
# the inferior is tested against it.
_AUTO_DEREF_PATTERNS = [
    re.compile(r"^alloc::vec::Vec<"),
]

# Memoised per-reference-type decision. Without this, every value GDB
# prints (every local, every struct field, every hover) pays the cost of
# stringifying its target type just so the regex can reject it, making the
# debugger slow.
# Cleared on inferior reload because Type object identities don't
# survive re-parse of DWARF.
_decision_cache = {}  # id(ref_type) -> bool


def _clear_decision_cache(_event=None):
    _decision_cache.clear()


try:
    gdb.events.new_objfile.connect(_clear_decision_cache)
except Exception:
    pass


class _AutoDerefPrinter:
    """Thin forwarder to the pointee's default visualizer."""

    def __init__(self, delegate):
        self._delegate = delegate

    def to_string(self):
        return self._delegate.to_string()

    def children(self):
        if hasattr(self._delegate, "children"):
            return self._delegate.children()
        return iter(())

    def display_hint(self):
        if hasattr(self._delegate, "display_hint"):
            return self._delegate.display_hint()
        return None


def _ref_lookup(val):
    t = val.type.strip_typedefs()
    if t.code not in (gdb.TYPE_CODE_REF, gdb.TYPE_CODE_PTR):
        return None
    cache_key = id(t)
    matches = _decision_cache.get(cache_key)
    if matches is None:
        try:
            target_name = str(t.target().strip_typedefs())
        except gdb.error:
            _decision_cache[cache_key] = False
            return None
        matches = any(p.match(target_name) for p in _AUTO_DEREF_PATTERNS)
        _decision_cache[cache_key] = matches
    if not matches:
        return None
    try:
        inner = (
            val.referenced_value() if t.code == gdb.TYPE_CODE_REF else val.dereference()
        )
    except (gdb.error, gdb.MemoryError):
        return None
    delegate = gdb.default_visualizer(inner)
    if delegate is None:
        return None
    return _AutoDerefPrinter(delegate)


gdb.pretty_printers.append(_ref_lookup)
