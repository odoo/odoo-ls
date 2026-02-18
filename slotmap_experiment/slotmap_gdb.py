"""
GDB commands for inspecting slotmap::SlotMap values.

Usage:
    (gdb) source slotmap_gdb.py
    (gdb) slotmap_get sm k --slot-size 56 --type slotmap_experiment::Symbol
    (gdb) slotmap_dump sm --slot-size 56 --type slotmap_experiment::Symbol
    (gdb) slotmap_detect sm

Add to ~/.gdbinit for automatic loading:
    source /path/to/slotmap_gdb.py

Requires rust-gdb (or GDB with Rust pretty-printers loaded) for best output.
"""

import gdb
import struct


# --- SlotMap layout knowledge ---

# Path through Vec internals to the data pointer (Rust 1.91+)
def get_vec_base_ptr(vec_val):
    """Navigate Vec<T> internals to get the raw data pointer as int."""
    return int(vec_val['buf']['inner']['ptr']['pointer']['pointer'])


def get_vec_len(vec_val):
    """Get Vec length. Read as u32 to avoid issues with adjacent fields."""
    # vec_val['len'] is usize, read it directly
    return int(vec_val['len'])


def get_key_fields(key_val):
    """Extract (idx, version) from a slotmap key value."""
    # Keys are newtypes: ExampleKey(__0: KeyData { idx, version })
    keydata = key_val['__0']
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


def parse_args(arg_string, need_key=False):
    """Parse command arguments. Returns (sm_name, key_name, slot_size, type_name)."""
    args = arg_string.split()
    if len(args) < 1:
        return None

    sm_name = args[0]
    key_name = None
    slot_size = None
    type_name = None

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


# --- Commands ---

class SlotMapGet(gdb.Command):
    """Look up a value in a SlotMap by key.
    Usage: slotmap_get <map> <key> --slot-size N [--type TypeName]

    With --type, the value is cast and displayed using GDB pretty-printers,
    giving full struct/enum navigation (like the Variables panel in VS Code).

    Without --type, a raw hex dump is shown."""

    def __init__(self):
        super().__init__("slotmap_get", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        parsed = parse_args(arg, need_key=True)
        if parsed is None:
            raise gdb.GdbError(
                "Usage: slotmap_get <map> <key> --slot-size N [--type TypeName]"
            )
        sm_name, key_name, slot_size, type_name = parsed

        sm = gdb.parse_and_eval(sm_name)
        key = gdb.parse_and_eval(key_name)

        base_ptr = get_vec_base_ptr(sm['slots'])
        vec_len = get_vec_len(sm['slots'])
        idx, key_version = get_key_fields(key)

        if idx >= vec_len:
            raise gdb.GdbError(f"Invalid key: index {idx} >= len {vec_len}")

        if slot_size is None:
            raise gdb.GdbError(
                "Error: --slot-size is required. Use slotmap_detect to find it."
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
            # Store as convenience variable so it's navigatable in VS Code Watch panel.
            # Add $slot_value to Watch to expand/navigate the struct.
            gdb.set_convenience_variable('slot_value', value)
            print(value)
            print("  -> stored in $slot_value (add to Watch panel to navigate)")
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
    Usage: slotmap_dump <map> --slot-size N [--type TypeName] [--max M]"""

    def __init__(self):
        super().__init__("slotmap_dump", gdb.COMMAND_DATA)

    def invoke(self, arg, from_tty):
        args_list = arg.split()
        sm_name = args_list[0] if args_list else None
        slot_size = None
        type_name = None
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

        if not sm_name or not slot_size:
            raise gdb.GdbError(
                "Usage: slotmap_dump <map> --slot-size N [--type TypeName] [--max M]"
            )

        sm = gdb.parse_and_eval(sm_name)
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


# --- Registration ---

SlotMapGet()
SlotMapDump()
SlotMapDetect()
print("SlotMap GDB commands loaded: slotmap_get, slotmap_dump, slotmap_detect")
