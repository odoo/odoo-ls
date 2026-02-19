"""
GDB commands for inspecting slotmap::SlotMap values.

Usage:
    (gdb) source slotmap_gdb.py

    # Configure defaults (do this once):
    (gdb) slotmap_config --slot-size 56 --type slotmap_experiment::Symbol

    # Then just:
    (gdb) slotmap_get sm k          # result in $slot_k
    (gdb) slotmap_get sm l          # result in $slot_l
    (gdb) slotmap_dump sm
    (gdb) slotmap_detect sm

    # Or override defaults per-call:
    (gdb) slotmap_get sm k --slot-size 24 --type other::Type

Add to ~/.gdbinit for automatic loading:
    source /path/to/slotmap_gdb.py

Requires rust-gdb (or GDB with Rust pretty-printers loaded) for best output.
"""

import gdb
import re
import struct


# --- Defaults (set via slotmap_config) ---

_default_slot_size = None
_default_type_name = None
_map_var_name = None         # SlotMap variable name for the pretty-printer
_known_key_types = set()     # Type names recognized as slotmap keys
_in_children = False         # True while children() is resolving; prevents recursion


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


def sanitize_var_name(expr):
    """Turn an arbitrary GDB expression into a valid convenience variable name.
    E.g. '$slot_l.__0.__0.parent' -> 'slot_l___0___0_parent'"""
    # Remove leading $
    name = expr.lstrip('$')
    # Replace any non-alphanumeric/underscore chars with _
    name = re.sub(r'[^a-zA-Z0-9_]', '_', name)
    # Collapse multiple underscores
    name = re.sub(r'_+', '_', name)
    # Strip leading/trailing underscores
    name = name.strip('_')
    return name


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


def parse_args(arg_string, need_key=False):
    """Parse command arguments. Returns (sm_name, key_name, slot_size, type_name).
    Falls back to defaults from slotmap_config if not specified."""
    args = arg_string.split()
    if len(args) < 1:
        return None

    sm_name = args[0]
    key_name = None
    slot_size = _default_slot_size
    type_name = _default_type_name

    i = 1
    if need_key:
        if len(args) < 2:
            return None
        key_name = args[1]
        i = 2

    while i < len(args):
        if args[i] == '--slot-size' and i + 1 < len(args):
            slot_size = int(args[i + 1])
            i += 2
        elif args[i] == '--type' and i + 1 < len(args):
            type_name = args[i + 1]
            i += 2
        elif args[i] == '--max' and i + 1 < len(args):
            i += 2  # consumed by dump
        else:
            raise gdb.GdbError(f"Unknown argument: {args[i]}")

    return sm_name, key_name, slot_size, type_name


# --- Pretty-printer for slotmap keys ---

def slotmap_resolve(key_val):
    """Try to resolve a slotmap key to its value. Returns gdb.Value or None."""
    if _map_var_name is None:
        return None
    try:
        sm = gdb.parse_and_eval(_map_var_name)
        slot_size, val_type = get_slotmap_type_info(sm)
        if slot_size is None or val_type is None:
            return None
        base_ptr = get_vec_base_ptr(sm['slots'])
        vec_len = get_vec_len(sm['slots'])
        idx, key_version = get_key_fields(key_val)
        if idx >= vec_len:
            return None
        version_offset = detect_version_offset(base_ptr, slot_size)
        slot_addr = base_ptr + idx * slot_size
        slot_version = read_u32_at(slot_addr + version_offset)
        if slot_version % 2 == 0 or slot_version != key_version:
            return None
        ptr = gdb.Value(slot_addr).cast(val_type.pointer())
        return ptr.dereference()
    except Exception:
        return None


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
            return f"Key(idx={idx}, v={version})"
        except Exception:
            return "<key>"  # safe static string; str(self.val) would recurse

    def children(self):
        global _in_children
        _in_children = True  # suppress printer on nested keys during resolution
        try:
            resolved = slotmap_resolve(self.val)
            if resolved is not None:
                yield ("[value]", resolved)
        except Exception:
            pass


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
            return f"Some(Key(idx={idx}, v={version}))"
        except Exception:
            return "<option key>"  # safe static string; str(self.val) would recurse

    def children(self):
        global _in_children
        _in_children = True  # suppress printer on nested keys during resolution
        if is_option_none(self.val):
            return
        try:
            key = unwrap_option(self.val)
            resolved = slotmap_resolve(key)
            if resolved is not None:
                yield ("[value]", resolved)
        except Exception:
            pass


def slotmap_type_lookup(val):
    """GDB pretty-printer lookup function. Returns a printer if the type
    is a known slotmap key or Option<key>, otherwise None (fall through
    to default printers)."""
    if _in_children or not _known_key_types:
        return None
    type_name = str(val.type.strip_typedefs())
    for kt in _known_key_types:
        if kt in type_name:
            if 'Option' in type_name:
                return SlotMapOptionKeyPrinter(val)
            elif '<' not in type_name:
                return SlotMapKeyPrinter(val)
    return None


# Register the lookup function globally (checked before objfile printers)
gdb.pretty_printers.append(slotmap_type_lookup)


# --- Commands ---

class SlotMapGet(gdb.Command):
    """Look up a value in a SlotMap by key.
    Usage: slotmap_get <map> <key> [--slot-size N] [--type TypeName]

    Slot size and value type are auto-detected from debug info.
    Result is stored in $slot (and $slot_<key>) for Watch panel navigation."""

    def __init__(self):
        super().__init__("slotmap_get", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        parsed = parse_args(arg, need_key=True)
        if parsed is None:
            raise gdb.GdbError("Usage: slotmap_get <map> <key-expr>")
        sm_name, key_expr, slot_size, type_name = parsed

        sm = gdb.parse_and_eval(sm_name)
        key = gdb.parse_and_eval(key_expr)

        # Check for Option::None before trying to extract key fields
        if is_option_none(key):
            print(f"Key expression '{key_expr}' is None (no parent/target)")
            return

        # Auto-detect from debug info if not overridden
        auto_slot_size, auto_val_type = get_slotmap_type_info(sm)
        if slot_size is None:
            slot_size = auto_slot_size
        if type_name is None and auto_val_type is not None:
            type_name = auto_val_type.name

        base_ptr = get_vec_base_ptr(sm['slots'])
        vec_len = get_vec_len(sm['slots'])
        idx, key_version = get_key_fields(key)

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

        # Build a clean convenience variable name from the key expression
        var_suffix = sanitize_var_name(key_expr)

        if type_name:
            # Cast to the value type — GDB pretty-printers handle the rest
            val_type = gdb.lookup_type(type_name)
            ptr = gdb.Value(slot_addr).cast(val_type.pointer())
            value = ptr.dereference()
            # Store in $slot (add to Watch once, always shows last lookup)
            # and in $slot_<suffix> for comparing multiple lookups.
            gdb.set_convenience_variable('slot', value)
            gdb.set_convenience_variable(f"slot_{var_suffix}", value)
            print(f"  → $slot, $slot_{var_suffix}")
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
    Usage: slotmap_dump <map> [--slot-size N] [--type TypeName] [--max M]

    Slot size and value type are auto-detected from debug info."""

    def __init__(self):
        super().__init__("slotmap_dump", gdb.COMMAND_DATA)

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
            raise gdb.GdbError("Usage: slotmap_dump <map> [--max M]")

        sm = gdb.parse_and_eval(sm_name)

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


class SlotMapDetect(gdb.Command):
    """Detect slot size by examining memory patterns.
    Usage: slotmap_detect <map>

    Requires at least 2 elements in the map for reliable detection."""

    def __init__(self):
        super().__init__("slotmap_detect", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        sm_name = arg.strip()
        if not sm_name:
            raise gdb.GdbError("Usage: slotmap_detect <map>")

        sm = gdb.parse_and_eval(sm_name)
        base_ptr = get_vec_base_ptr(sm['slots'])
        vec_len = get_vec_len(sm['slots'])
        num_elems = int(sm['num_elems'])

        if num_elems == 0:
            raise gdb.GdbError(
                "SlotMap is empty, can't detect slot size. "
                "Insert at least one element."
            )

        # Read a generous chunk of memory
        chunk_size = min(1024, 256 * max(vec_len, 4))
        inferior = gdb.selected_inferior()
        data = bytes(inferior.read_memory(base_ptr, chunk_size))

        def read_u32_at_off(off):
            if off + 4 > len(data):
                return None
            return struct.unpack('<I', data[off:off + 4])[0]

        # Slot 0: vacant, version=0. Slot 1: first insert, version=1.
        # Scan for version=1 at 4-byte aligned offsets, paired with
        # version=0 one slot_size earlier.
        candidates = []
        for off in range(8, chunk_size - 3, 4):
            if read_u32_at_off(off) != 1:
                continue
            for v_off in range(4, off, 4):
                if read_u32_at_off(v_off) != 0:
                    continue
                slot_size = off - v_off
                if slot_size < 8 or slot_size % 4 != 0:
                    continue
                # Structural constraint: version is at slot_size-8 or slot_size-4
                if v_off != slot_size - 8 and v_off != slot_size - 4:
                    continue
                # Validate with slot 2 if available
                if vec_len >= 3 and num_elems >= 2:
                    v2 = read_u32_at_off(v_off + 2 * slot_size)
                    if v2 is None or v2 % 2 != 1:
                        continue
                candidates.append((slot_size, v_off))

        if not candidates:
            raise gdb.GdbError(
                "Could not detect slot size. Please specify --slot-size manually."
            )

        seen = {}
        for ss, vo in candidates:
            if ss not in seen:
                seen[ss] = vo

        print(f"SlotMap '{sm_name}': {num_elems} elements, {vec_len} slots")
        if len(seen) == 1:
            ss, vo = next(iter(seen.items()))
            print(f"Detected slot_size = {ss}  (version at offset {vo})")
        else:
            print("Candidate slot sizes:")
            for ss, vo in sorted(seen.items()):
                print(f"  slot_size = {ss}  (version at offset {vo})")


class SlotMapConfig(gdb.Command):
    """Set defaults for slotmap commands and enable inline key resolution.
    Usage: slotmap_config --map <variable> [--slot-size N] [--type TypeName]

    --map enables the pretty-printer: expanding a key field in the
    Variables/Watch panel will automatically show the resolved value.
    The key type is auto-detected from the SlotMap's type info.

    Example:
        slotmap_config --map sm
        # Now 'parent: Some(Key(idx=0, v=1))' is expandable → shows the Symbol"""

    def __init__(self):
        super().__init__("slotmap_config", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        global _default_slot_size, _default_type_name, _map_var_name
        args = arg.split()
        i = 0
        while i < len(args):
            if args[i] == '--slot-size' and i + 1 < len(args):
                _default_slot_size = int(args[i + 1])
                i += 2
            elif args[i] == '--type' and i + 1 < len(args):
                _default_type_name = args[i + 1]
                i += 2
            elif args[i] == '--map' and i + 1 < len(args):
                _map_var_name = args[i + 1]
                i += 2
            else:
                raise gdb.GdbError(f"Unknown argument: {args[i]}")

        # Auto-detect key type from the SlotMap if --map was given
        if _map_var_name:
            try:
                sm = gdb.parse_and_eval(_map_var_name)
                # SlotMap<K, V> — K is template_argument(0)
                key_type = sm.type.template_argument(0)
                key_type_name = str(key_type)
                _known_key_types.add(key_type_name)
                print(f"Registered pretty-printer for key type: {key_type_name}")
            except gdb.error as e:
                print(f"Warning: could not detect key type from '{_map_var_name}': {e}")
                print("Use slotmap_config when the map variable is in scope.")

        print(f"SlotMap defaults: map={_map_var_name}, "
              f"slot_size={_default_slot_size}, type={_default_type_name}")


# --- Registration ---

SlotMapGet()
SlotMapDump()
SlotMapDetect()
SlotMapConfig()
print("SlotMap GDB commands loaded: slotmap_get, slotmap_dump, slotmap_detect, slotmap_config")
