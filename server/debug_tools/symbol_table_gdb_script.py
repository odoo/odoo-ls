"""
GDB tools for inspecting odoo-ls's arena `SymbolTable`.

The script bootstraps itself from the debug-global pointer
`ODOOLS_DEBUG_SYMBOL_TABLE` (installed by `SymbolTable::install_debug_ptr`
in the Rust side), so no manual configuration is required: every slotmap
field on `SymbolTable` is auto-registered on first use, and slotmap-key
locals (e.g. `FileKey`, `Option<VariableKey>`) are pretty-printed with
their `[symbol]` resolved inline.

Commands:
    (gdb) st_inspect              # summarize the active SymbolTable
    (gdb) st_get  <map> <idx> <v> # resolve a slot by idx+version (result in $slot)
    (gdb) st_dump <map> [--max N] # walk every occupied slot
    (gdb) st_config [--slot-size N] [--type T] [--map V]
                                  # legacy overrides; rarely needed now

`<map>` is a SymbolTable field name (e.g. `files`, `variables`) — looked
up through the debug-global — or any in-scope slotmap expression.

Add to ~/.gdbinit to autoload:
    source /path/to/symbol_table_gdb_script.py

Add to .vscode/launch.json and settings.json (for rust-analyzer) debug
configurations with GDB:
    "setupCommands": [
        {
            "description": "Load SymbolTable GDB script",
            "text": "source ${workspaceFolder}/server/debug_tools/symbol_table_gdb_script.py",
            "ignoreFailures": false
        },
    ]

Requires rust-gdb (or GDB with Rust pretty-printers loaded). Debug build
only — `ODOOLS_DEBUG_SYMBOL_TABLE` is gated on `cfg(debug_assertions)`.
"""

import gdb
import re
import struct


# --- Defaults (set via st_config) ---

_default_slot_size = None
_default_type_name = None
_key_to_map = {}             # key_type_name → map variable expression
_in_children = False         # True while children() is resolving; prevents recursion


# --- Debug-global bootstrap ---

_SYMBOL_TABLE_TYPE = "odoo_ls_server::core::symbols::storage::SymbolTable"
_SYMBOL_TABLE_GLOBAL = "ODOOLS_DEBUG_SYMBOL_TABLE"

# Cached lookups — both the linker-symbol address and the dereferenced
# `SymbolTable` value stay valid for an inferior's lifetime (the static's
# address is fixed, and the SymbolTable is reassigned in-place at
# SyncOdoo::reset so the pointer doesn't move). `gdb.events.new_objfile`
# clears them on inferior reload.
_cached_st_addr = None
_cached_st = None


def _invalidate_st_cache(_event=None):
    global _cached_st_addr, _cached_st
    _cached_st_addr = None
    _cached_st = None
    _key_to_map.clear()


try:
    gdb.events.new_objfile.connect(_invalidate_st_cache)
except (AttributeError, gdb.error):
    pass  # Older GDBs without this event — cache will just be stickier.


def _lookup_global_addr(name):
    """Return the address of a linker symbol, or None. Works across GDB
    versions: `gdb.lookup_minimal_symbol` is GDB 14+, so fall back to
    parsing `info address` output, which exists in every supported GDB."""
    if hasattr(gdb, 'lookup_minimal_symbol'):
        msym = gdb.lookup_minimal_symbol(name)
        if msym is None:
            return None
        return int(msym.value().address)
    try:
        out = gdb.execute(f"info address {name}", to_string=True)
    except gdb.error:
        return None
    m = re.search(r'0x[0-9a-fA-F]+', out)
    if m is None:
        return None
    return int(m.group(0), 16)


def _register_slotmaps_from(st):
    """Walk `SymbolTable`'s fields and populate `_key_to_map` with each
    slotmap's key type → field name mapping. Skips non-slotmap fields
    (e.g. `ext_symbols`) by requiring `SlotMap<` in the field's type."""
    for field in st.type.fields():
        try:
            sub = st[field.name]
        except (gdb.error, KeyError):
            continue
        if 'SlotMap<' not in str(sub.type):
            continue
        try:
            key_type = sub.type.template_argument(0)
        except gdb.error:
            continue
        _key_to_map[str(key_type)] = field.name


def get_symbol_table():
    """Fetch the active SymbolTable via the debug-global pointer installed
    by `SymbolTable::install_debug_ptr`. Returns a `gdb.Value` of the
    dereferenced SymbolTable, or None when the pointer is null or the
    symbol isn't present (release build, pre-init, etc.).

    Cached at module scope; `_invalidate_st_cache` clears it on
    `new_objfile`. On the first successful bootstrap, populates
    `_key_to_map` with every slotmap field on the table, so key resolution
    works without any manual `st_config --map` calls."""
    global _cached_st_addr, _cached_st
    if _cached_st is not None:
        return _cached_st
    if _cached_st_addr is None:
        _cached_st_addr = _lookup_global_addr(_SYMBOL_TABLE_GLOBAL)
        if _cached_st_addr is None:
            return None
    # Read the *value* stored at the static's address (the pointer it holds).
    data = gdb.selected_inferior().read_memory(_cached_st_addr, 8)
    target = struct.unpack('<Q', bytes(data))[0]
    if target == 0:
        return None  # Pre-init: don't cache, retry on next call.
    try:
        st_type = gdb.lookup_type(_SYMBOL_TABLE_TYPE)
    except gdb.error:
        return None
    st = gdb.Value(target).cast(st_type.pointer()).dereference()
    _cached_st = st
    if not _key_to_map:
        _register_slotmaps_from(st)
    return st


# --- SlotMap layout knowledge ---

# Path through Vec internals to the data pointer (Rust 1.91+)
def get_vec_base_ptr(vec_val):
    """Navigate Vec<T> internals to get the raw data pointer as int."""
    return int(vec_val['buf']['inner']['ptr']['pointer']['pointer'])


def get_vec_len(vec_val):
    """Get Vec length. Read as u32 to avoid issues with adjacent fields."""
    # vec_val['len'] is usize, read it directly
    return int(vec_val['len'])


def unwrap_option(val):
    """If val is an Option<T>, return the inner T (or None if it's None variant).
    If val is not an Option, return it unchanged."""
    type_name = str(val.type.strip_typedefs())
    if 'Option' not in type_name:
        return val

    # Rust's Option with niche optimization (e.g. Option<Key> where Key
    # contains NonZeroU32) has no discriminant — None is represented by 0.
    # GDB with Rust pretty-printers exposes the variant as a field.
    # Try common GDB representations:
    try:
        # Some compilers/versions: variant field accessible directly
        # Check if this looks like None by trying to read the inner value
        inner = val['__0']
        # Verify it's not a null/zero key (None with niche optimization)
        # by checking if we can extract valid key data from it
        return inner
    except gdb.error:
        pass

    # Try enum variant access patterns
    for field_name in ['value', 'Some', '__0']:
        try:
            inner = val[field_name]
            # For Some(v), the value is often one more level in
            try:
                return inner['__0']
            except gdb.error:
                return inner
        except gdb.error:
            continue

    return val


def get_key_fields(key_val):
    """Extract (idx, version) from a slotmap key value.
    Handles Option<Key> wrapping automatically."""
    # Unwrap Option if present
    key_val = unwrap_option(key_val)

    # Keys are newtypes: ExampleKey(__0: KeyData { idx, version })
    # Try direct KeyData access first, then unwrap one newtype layer
    keydata = None
    for candidate in [key_val, ]:
        try:
            kd = candidate['__0']
            # Verify it has idx and version (it's KeyData)
            _ = kd['idx']
            keydata = kd
            break
        except gdb.error:
            continue

    if keydata is None:
        # Maybe key_val IS the KeyData directly
        try:
            _ = key_val['idx']
            keydata = key_val
        except gdb.error:
            raise gdb.GdbError(
                f"Cannot extract key fields from type {key_val.type}. "
                "Expected a slotmap key or Option<Key>."
            )

    idx = int(keydata['idx'])
    # version is NonZeroU32 which wraps: version.__0.__0 or just version
    version_field = keydata['version']
    # Navigate through NonZero wrapper
    try:
        version = int(version_field['__0']['__0'])
    except gdb.error:
        try:
            version = int(version_field['__0'])
        except gdb.error:
            version = int(version_field)
    return idx, version


def read_u32_at(addr):
    """Read a u32 from the inferior's memory."""
    inferior = gdb.selected_inferior()
    data = inferior.read_memory(addr, 4)
    return struct.unpack('<I', bytes(data))[0]


def get_slotmap_type_info(sm_val):
    """Extract slot_size and value type from a SlotMap's debug info.
    Returns (slot_size, val_type) or (None, None) if extraction fails."""
    try:
        slot_type = sm_val['slots'].type.template_argument(0)
        slot_size = slot_type.sizeof
        val_type = slot_type.template_argument(0)
        return slot_size, val_type
    except gdb.error:
        return None, None


def detect_version_offset(base_ptr, slot_size):
    """Auto-detect version field offset within a Slot.

    The version u32 is right after the value union, so it's at either:
      slot_size - 8  (8-byte aligned value types, 4 bytes padding)
      slot_size - 4  (4-byte aligned value types, no padding)

    Slot 0 is always vacant with version=0; we check which offset holds 0.
    """
    for offset in [slot_size - 8, slot_size - 4]:
        if offset > 0:
            v = read_u32_at(base_ptr + offset)
            if v == 0:
                return offset
    return slot_size - 8  # fallback


def is_option_none(val):
    """Check if a value is Option::None."""
    type_name = str(val.type.strip_typedefs())
    if 'Option' not in type_name:
        return False
    # For niche-optimized Option<Key> (where Key contains NonZeroU32),
    # None is represented by the niche value (0 in the NonZero field).
    # Try to read the inner value and check if it's zero/null.
    try:
        inner = val['__0']
        # Check if it looks like a key with version=0 (None niche)
        try:
            kd = inner['__0']  # KeyData inside newtype
            version_field = kd['version']
            try:
                v = int(version_field['__0']['__0'])
            except gdb.error:
                try:
                    v = int(version_field['__0'])
                except gdb.error:
                    v = int(version_field)
            return v == 0  # NonZeroU32 == 0 means None
        except gdb.error:
            pass
    except gdb.error:
        pass
    # If niche detection failed, assume not-None
    # (worst case: we try to resolve and fail gracefully).
    # Do NOT use str(val) here — it triggers our to_string() → infinite recursion.
    return False


# --- Pretty-printer for slotmap keys ---

def _resolve_map(name_or_expr):
    """Look up a slotmap by either a `SymbolTable` field name (fast path,
    goes through the debug-global bootstrap) or an arbitrary GDB
    expression (legacy path for `st_config --map <var>`)."""
    st = get_symbol_table()
    if st is not None:
        try:
            return st[name_or_expr]
        except (gdb.error, KeyError):
            pass
    try:
        return gdb.parse_and_eval(name_or_expr)
    except gdb.error:
        return None


def slotmap_resolve(key_val):
    """Try to resolve a slotmap key. Returns `(status, value)`:
      ("valid", gdb.Value)  — slot occupied and version matches
      ("expired", None)     — slot vacant, out of range, or version mismatch
      ("unknown", None)     — no registered map for this key type, or any
                              other extraction failure (don't claim status)"""
    if not _key_to_map:
        return ("unknown", None)
    # Find the map expression for this key's type
    key_type = str(key_val.type.strip_typedefs())
    map_expr = None
    for kt, expr in _key_to_map.items():
        if kt in key_type or key_type in kt:
            map_expr = expr
            break
    if map_expr is None:
        return ("unknown", None)
    try:
        sm = _resolve_map(map_expr)
        if sm is None:
            return ("unknown", None)
        slot_size, val_type = get_slotmap_type_info(sm)
        if slot_size is None or val_type is None:
            return ("unknown", None)
        base_ptr = get_vec_base_ptr(sm['slots'])
        vec_len = get_vec_len(sm['slots'])
        idx, key_version = get_key_fields(key_val)
        if idx >= vec_len:
            return ("expired", None)
        version_offset = detect_version_offset(base_ptr, slot_size)
        slot_addr = base_ptr + idx * slot_size
        slot_version = read_u32_at(slot_addr + version_offset)
        if slot_version % 2 == 0 or slot_version != key_version:
            return ("expired", None)
        ptr = gdb.Value(slot_addr).cast(val_type.pointer())
        return ("valid", ptr.dereference())
    except Exception:
        return ("unknown", None)


class SlotMapKeyPrinter:
    """Pretty-printer for slotmap key types.
    Shows key info and resolves the value as an expandable child."""

    def __init__(self, val):
        self.val = val

    def to_string(self):
        global _in_children
        _in_children = False  # reset flag so this top-level key can resolve
        try:
            idx, version = get_key_fields(self.val)
            type_name = str(self.val.type.strip_typedefs()).rsplit("::", 1)[-1]
            return f"{type_name}(idx={idx}, v={version})"
        except Exception:
            return "<key>"  # safe static string; str(self.val) would recurse

    def children(self):
        global _in_children
        prev = _in_children
        _in_children = True  # suppress printer on nested keys during resolution
        try:
            status, resolved = slotmap_resolve(self.val)
            if status == "valid":
                yield ("[symbol]", resolved)
            elif status == "expired":
                # None as the value: GDB renders the row with the name only,
                # no expandable bytes (a Python str became a char[N] earlier).
                yield ("[expired]", None)
        except Exception:
            pass
        finally:
            _in_children = prev  # always restore, even if GDB abandons us


class SlotMapOptionKeyPrinter:
    """Pretty-printer for Option<Key> that resolves Some keys inline."""

    def __init__(self, val):
        self.val = val

    def to_string(self):
        global _in_children
        _in_children = False  # reset flag so this top-level key can resolve
        if is_option_none(self.val):
            return "None"
        try:
            key = unwrap_option(self.val)
            idx, version = get_key_fields(key)
            inner_type = str(key.type.strip_typedefs()).rsplit("::", 1)[-1]
            return f"Some({inner_type}(idx={idx}, v={version}))"
        except Exception:
            return "<option key>"  # safe static string; str(self.val) would recurse

    def children(self):
        global _in_children
        prev = _in_children
        _in_children = True  # suppress printer on nested keys during resolution
        try:
            if is_option_none(self.val):
                return
            key = unwrap_option(self.val)
            status, resolved = slotmap_resolve(key)
            if status == "valid":
                yield ("[symbol]", resolved)
            elif status == "expired":
                yield ("[expired]", None)
        except Exception:
            pass
        finally:
            _in_children = prev  # always restore, even if GDB abandons us


def slotmap_type_lookup(val):
    """GDB pretty-printer lookup function. Returns a printer if the type
    is a known slotmap key or Option<key>, otherwise None (fall through
    to default printers)."""
    if _in_children:
        return None
    if not _key_to_map:
        # Lazy bootstrap: first displayed value triggers slotmap registration
        # so printers light up without needing an explicit command.
        get_symbol_table()
    if not _key_to_map:
        return None
    type_name = str(val.type.strip_typedefs())
    for kt in _key_to_map:
        if kt in type_name:
            if 'Option' in type_name:
                return SlotMapOptionKeyPrinter(val)
            elif '<' not in type_name:
                return SlotMapKeyPrinter(val)
    return None


# Register the lookup function globally (checked before objfile printers)
gdb.pretty_printers.append(slotmap_type_lookup)


# --- Commands ---


class SymbolTableInspect(gdb.Command):
    """Summarize the active SymbolTable reached via ODOOLS_DEBUG_SYMBOL_TABLE.

    Usage: st_inspect

    Prints the SymbolTable's address and each slotmap's occupancy. Useful
    as a sanity check that the debug global is installed and reachable."""

    def __init__(self):
        super().__init__("st_inspect", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        st = get_symbol_table()
        if st is None:
            print(
                f"SymbolTable unavailable: {_SYMBOL_TABLE_GLOBAL} is null or "
                "absent. Is this a debug build? Has SyncOdoo::init run?"
            )
            return
        print(f"SymbolTable @ {st.address}")
        for field in st.type.fields():
            try:
                sub = st[field.name]
            except (gdb.error, KeyError):
                continue
            if 'SlotMap<' not in str(sub.type):
                continue
            num_elems = int(sub['num_elems'])
            slots_len = int(sub['slots']['len'])
            key_type = sub.type.template_argument(0)
            print(
                f"  {field.name:<16} {num_elems:>7} elems  "
                f"{slots_len:>7} slots  [{key_type}]"
            )




class SlotMapGet(gdb.Command):
    """Look up a value in a SlotMap by (idx, version).
    Usage: st_get <map> <idx> <version> [--slot-size N] [--type TypeName]

    `<map>` is a SymbolTable field name (resolved via the debug-global)
    or any in-scope slotmap expression. `<idx>` and `<version>` are the
    integers shown on a key in the Watch panel (e.g. `FileKey(idx=55, v=1)`
    → `st_get files 55 1`). Slot size and value type are auto-detected
    from debug info; pass `--slot-size`/`--type` only to override. Result
    is stored in $slot (and $slot_<idx>_<version>) for Watch panel
    navigation."""

    def __init__(self):
        super().__init__("st_get", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        args = arg.split()
        slot_size = _default_slot_size
        type_name = _default_type_name
        positional = []
        i = 0
        while i < len(args):
            if args[i] == '--slot-size' and i + 1 < len(args):
                slot_size = int(args[i + 1])
                i += 2
            elif args[i] == '--type' and i + 1 < len(args):
                type_name = args[i + 1]
                i += 2
            else:
                positional.append(args[i])
                i += 1

        if len(positional) != 3:
            raise gdb.GdbError("Usage: st_get <map> <idx> <version>")
        sm_name = positional[0]
        try:
            idx = int(positional[1])
            key_version = int(positional[2])
        except ValueError:
            raise gdb.GdbError("idx and version must be integers")

        sm = _resolve_map(sm_name)
        if sm is None:
            raise gdb.GdbError(f"Could not resolve map '{sm_name}'")

        # Auto-detect from debug info if not overridden
        auto_slot_size, auto_val_type = get_slotmap_type_info(sm)
        if slot_size is None:
            slot_size = auto_slot_size
        if type_name is None and auto_val_type is not None:
            type_name = auto_val_type.name

        base_ptr = get_vec_base_ptr(sm['slots'])
        vec_len = get_vec_len(sm['slots'])

        if idx >= vec_len:
            raise gdb.GdbError(f"Invalid key: index {idx} >= len {vec_len}")

        if slot_size is None:
            raise gdb.GdbError(
                "Could not detect slot size. Use --slot-size N."
            )

        version_offset = detect_version_offset(base_ptr, slot_size)
        slot_addr = base_ptr + idx * slot_size
        slot_version = read_u32_at(slot_addr + version_offset)
        occupied = slot_version % 2 == 1
        if not occupied:
            print(f"Slot {idx} is VACANT (version={slot_version}, "
                  f"key version={key_version})")
            return

        if slot_version != key_version:
            print(f"STALE KEY: slot version={slot_version}, "
                  f"key version={key_version} (slot has been reused)")
            return

        print(f"Slot {idx}: OCCUPIED (version={slot_version})")

        if type_name:
            # Cast to the value type — GDB pretty-printers handle the rest
            val_type = gdb.lookup_type(type_name)
            ptr = gdb.Value(slot_addr).cast(val_type.pointer())
            value = ptr.dereference()
            # Store in $slot (add to Watch once, always shows last lookup)
            # and in $slot_<idx>_<version> for comparing multiple lookups.
            gdb.set_convenience_variable('slot', value)
            gdb.set_convenience_variable(f"slot_{idx}_{key_version}", value)
            print(f"  → $slot, $slot_{idx}_{key_version}")
            print(value)
        else:
            # Raw hex dump
            value_size = version_offset
            inferior = gdb.selected_inferior()
            data = bytes(inferior.read_memory(slot_addr, value_size))
            for off in range(0, len(data), 8):
                chunk = data[off:off + 8]
                hex_str = " ".join(f"{b:02x}" for b in chunk)
                ascii_str = "".join(
                    chr(b) if 32 <= b < 127 else "." for b in chunk
                )
                print(f"  +{off:4d}: {hex_str:<24s}  {ascii_str}")


class SlotMapDump(gdb.Command):
    """Dump all occupied slots in a SlotMap.
    Usage: st_dump <map> [--slot-size N] [--type TypeName] [--max M]

    `<map>` is a SymbolTable field name (resolved via the debug-global)
    or any in-scope slotmap expression. Slot size and value type are
    auto-detected from debug info."""

    def __init__(self):
        super().__init__("st_dump", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        args_list = arg.split()
        sm_name = args_list[0] if args_list else None
        slot_size = _default_slot_size
        type_name = _default_type_name
        max_display = 20

        i = 1
        while i < len(args_list):
            if args_list[i] == '--slot-size' and i + 1 < len(args_list):
                slot_size = int(args_list[i + 1])
                i += 2
            elif args_list[i] == '--type' and i + 1 < len(args_list):
                type_name = args_list[i + 1]
                i += 2
            elif args_list[i] == '--max' and i + 1 < len(args_list):
                max_display = int(args_list[i + 1])
                i += 2
            else:
                i += 1

        if not sm_name:
            raise gdb.GdbError("Usage: st_dump <map> [--max M]")

        sm = _resolve_map(sm_name)
        if sm is None:
            raise gdb.GdbError(f"Could not resolve map '{sm_name}'")

        # Auto-detect from debug info if not overridden
        auto_slot_size, auto_val_type = get_slotmap_type_info(sm)
        if slot_size is None:
            slot_size = auto_slot_size
        if type_name is None and auto_val_type is not None:
            type_name = auto_val_type.name

        if slot_size is None:
            raise gdb.GdbError(
                "Could not detect slot size. Use --slot-size N."
            )

        base_ptr = get_vec_base_ptr(sm['slots'])
        vec_len = get_vec_len(sm['slots'])
        num_elems = int(sm['num_elems'])
        version_offset = detect_version_offset(base_ptr, slot_size)

        val_type = gdb.lookup_type(type_name) if type_name else None

        print(f"SlotMap '{sm_name}': {num_elems} elements, "
              f"{vec_len} slots allocated")

        displayed = 0
        for idx in range(vec_len):
            if displayed >= max_display:
                print(f"  ... ({vec_len - idx} more slots)")
                break

            slot_addr = base_ptr + idx * slot_size
            version = read_u32_at(slot_addr + version_offset)
            if version % 2 == 0:
                continue  # vacant

            print(f"\n  [{idx}] version={version}")
            if val_type:
                ptr = gdb.Value(slot_addr).cast(val_type.pointer())
                print(f"  {ptr.dereference()}")
            displayed += 1


class SlotMapConfig(gdb.Command):
    """Override defaults and/or register an extra slotmap by hand.
    Usage: st_config [--map <var>] [--slot-size N] [--type TypeName]

    Rarely needed: the printer auto-bootstraps from the debug-global
    `ODOOLS_DEBUG_SYMBOL_TABLE`, so all SymbolTable slotmaps are
    registered on first use. `--map` is for slotmaps that live outside
    the SymbolTable (e.g. an experimental local). `--slot-size`/`--type`
    only matter if `get_slotmap_type_info` can't read the debug info."""

    def __init__(self):
        super().__init__("st_config", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        global _default_slot_size, _default_type_name
        args = arg.split()
        map_var = None
        i = 0
        while i < len(args):
            if args[i] == '--slot-size' and i + 1 < len(args):
                _default_slot_size = int(args[i + 1])
                i += 2
            elif args[i] == '--type' and i + 1 < len(args):
                _default_type_name = args[i + 1]
                i += 2
            elif args[i] == '--map' and i + 1 < len(args):
                map_var = args[i + 1]
                i += 2
            else:
                raise gdb.GdbError(f"Unknown argument: {args[i]}")

        # Register key type → map mapping if --map was given
        if map_var:
            try:
                sm = gdb.parse_and_eval(map_var)
                key_type = sm.type.template_argument(0)
                key_type_name = str(key_type)
                _key_to_map[key_type_name] = map_var
                print(f"Registered: {key_type_name} \u2192 {map_var}")
            except gdb.error as e:
                print(f"Warning: could not detect key type from '{map_var}': {e}")
                print("Use st_config when the map variable is in scope.")

        print(f"SlotMap config: maps={_key_to_map}, "
              f"slot_size={_default_slot_size}, type={_default_type_name}")


# --- Registration ---

SlotMapGet()
SlotMapDump()
SlotMapConfig()
SymbolTableInspect()
print("SymbolTable GDB commands loaded: st_inspect, st_get, st_dump, st_config")
