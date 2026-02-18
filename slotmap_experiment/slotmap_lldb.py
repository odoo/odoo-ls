"""
LLDB commands for inspecting slotmap::SlotMap values.

Usage:
    (lldb) command script import slotmap_lldb.py
    (lldb) slotmap_get sm k
    (lldb) slotmap_get sm k --slot-size 24
    (lldb) slotmap_dump sm --slot-size 24

Add to ~/.lldbinit for automatic loading:
    command script import /path/to/slotmap_lldb.py
"""

import lldb


# --- Helpers to read from the inferior process ---

def read_u32(process, addr):
    err = lldb.SBError()
    val = process.ReadUnsignedFromMemory(addr, 4, err)
    if err.Fail():
        return None
    return val


def read_u64(process, addr):
    err = lldb.SBError()
    val = process.ReadUnsignedFromMemory(addr, 8, err)
    if err.Fail():
        return None
    return val


def read_bytes(process, addr, size):
    err = lldb.SBError()
    data = process.ReadMemory(addr, size, err)
    if err.Fail():
        return None
    return data


def eval_unsigned(frame, expr):
    """Evaluate an expression and return its unsigned value, or None."""
    val = frame.EvaluateExpression(expr)
    if not val.IsValid() or val.GetError().Fail():
        return None
    return val.GetValueAsUnsigned()


# --- SlotMap layout knowledge ---

# Vec internal path to the data pointer (Rust 1.91+)
VEC_PTR_PATH = ".slots.buf.inner.ptr.pointer.pointer"


def detect_version_offset(process, base_ptr, slot_size):
    """
    Auto-detect where the version u32 lives within a Slot.

    Slot 0 is always the freelist head (vacant, version=0).
    The version field is right after the union, which means either:
      - slot_size - 8  (for 8-byte aligned value types, 4 bytes padding after version)
      - slot_size - 4  (for 4-byte aligned value types, no padding)

    We check slot 0: whichever candidate offset holds 0 (even = vacant) is correct.
    """
    candidates = []
    if slot_size > 8:
        candidates.append(slot_size - 8)
    if slot_size > 4:
        candidates.append(slot_size - 4)

    for offset in candidates:
        v = read_u32(process, base_ptr + offset)
        if v == 0:
            return offset

    # Fallback: assume 8-byte aligned
    return slot_size - 8


def get_slotmap_info(frame, sm_name):
    """Extract base pointer and vec length from a SlotMap variable."""
    ptr_expr = f"(unsigned long long){sm_name}{VEC_PTR_PATH}"
    base_ptr = eval_unsigned(frame, ptr_expr)
    if base_ptr is None:
        return None, None, "Could not read slots base pointer"

    # Cast to u32: lldb misreads the usize field on some Rust versions,
    # picking up adjacent fields. SlotMap can't exceed u32::MAX slots anyway.
    vec_len = eval_unsigned(frame, f"(unsigned int){sm_name}.slots.len")
    if vec_len is None:
        return None, None, "Could not read slots length"

    return base_ptr, vec_len, None


def get_key_info(frame, key_name):
    """Extract idx and version from a slotmap key variable."""
    idx = eval_unsigned(frame, f"{key_name}.__0.idx")
    if idx is None:
        return None, None, f"Could not read {key_name}.__0.idx"

    version = eval_unsigned(frame, f"{key_name}.__0.version.__0.__0")
    if version is None:
        return None, None, f"Could not read {key_name}.__0.version"

    return idx, version, None


# --- Commands ---

def cmd_slotmap_get(debugger, command, result, internal_dict):
    """Look up a value in a SlotMap by key.
    Usage: slotmap_get <map> <key> [--slot-size N] [--type TypeName]"""
    args = command.split()
    if len(args) < 2:
        result.AppendMessage("Usage: slotmap_get <slotmap_var> <key_var> [--slot-size N] [--type TypeName]")
        return

    sm_name = args[0]
    key_name = args[1]
    slot_size = None
    value_type = None

    i = 2
    while i < len(args):
        if args[i] == "--slot-size" and i + 1 < len(args):
            slot_size = int(args[i + 1])
            i += 2
        elif args[i] == "--type" and i + 1 < len(args):
            value_type = args[i + 1]
            i += 2
        else:
            result.AppendMessage(f"Unknown argument: {args[i]}")
            return

    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()

    base_ptr, vec_len, err = get_slotmap_info(frame, sm_name)
    if err:
        result.AppendMessage(f"Error: {err}")
        return

    idx, key_version, err = get_key_info(frame, key_name)
    if err:
        result.AppendMessage(f"Error: {err}")
        return

    if idx >= vec_len:
        result.AppendMessage(f"Invalid key: index {idx} >= len {vec_len}")
        return

    if slot_size is None:
        result.AppendMessage(
            "Error: --slot-size is required.\n"
            "  Tip: add `println!(\"slot_size: {}\", std::mem::size_of::<slotmap::KeyData>());`\n"
            "  ... just kidding. Slot is private. Use slotmap_detect to figure it out,\n"
            "  or compute manually: align_up(max(size_of::<V>(), 4) + 4, align_of::<V>())"
        )
        return

    version_offset = detect_version_offset(process, base_ptr, slot_size)
    slot_addr = base_ptr + idx * slot_size
    slot_version = read_u32(process, slot_addr + version_offset)

    if slot_version is None:
        result.AppendMessage(f"Error: could not read memory at slot {idx}")
        return

    occupied = slot_version % 2 == 1
    version_match = slot_version == key_version

    if not occupied:
        result.AppendMessage(f"Slot {idx} is VACANT (version={slot_version}, key version={key_version})")
        return

    if not version_match:
        result.AppendMessage(
            f"STALE KEY: slot version={slot_version}, key version={key_version} (slot reused)"
        )
        return

    value_size = version_offset  # value occupies bytes 0..version_offset
    result.AppendMessage(f"Slot {idx}: OCCUPIED (version={slot_version})")
    result.AppendMessage(f"  slot address:  0x{slot_addr:x}")
    result.AppendMessage(f"  value address: 0x{slot_addr:x}  ({value_size} bytes)")

    if value_type:
        # Use CreateValueFromAddress to map the slot memory to the value type.
        # The Rust lldb scripts (loaded by rust-lldb) provide synthetic formatters
        # that handle enums, String, Vec, etc. — we use `frame var` style output
        # via the command interpreter so those formatters activate.
        val_type = target.FindFirstType(value_type)
        if not val_type.IsValid():
            result.AppendMessage(f"  Type '{value_type}' not found in debug info.")
            result.AppendMessage(f"  Hint: use fully qualified name, e.g. 'mycrate::MyStruct'")
            result.AppendMessage(f"  Falling back to hex dump...")
            _dump_hex(process, result, slot_addr, value_size)
            return

        sb_addr = lldb.SBAddress(slot_addr, target)
        val = target.CreateValueFromAddress("$slot_value", sb_addr, val_type)
        if not val.IsValid():
            result.AppendMessage(f"  Could not create value from address")
            _dump_hex(process, result, slot_addr, value_size)
            return

        # Use the Rust synthetic formatters by printing via the command interpreter.
        # `script` lets us call the formatters directly on our SBValue.
        result.AppendMessage(f"  value ({value_type}):")
        _print_sbvalue(result, val, indent=4)
    else:
        result.AppendMessage(f"  value memory:")
        _dump_hex(process, result, slot_addr, value_size)


def _print_value(result, val, indent=4, max_depth=5):
    """Recursively print an SBValue with its children, using synthetic formatters."""
    prefix = " " * indent

    # Get the summary (synthetic formatters produce these for String, Vec, etc.)
    summary = val.GetSummary()
    value = val.GetValue()
    type_name = val.GetTypeName() or ""
    name = val.GetName() or ""
    num_children = val.GetNumChildren()

    # Leaf node: has a value or summary but no children worth expanding
    if summary:
        result.AppendMessage(f"{prefix}{name}: {summary}")
        return
    if value and num_children == 0:
        result.AppendMessage(f"{prefix}{name} = {value}")
        return

    if max_depth <= 0:
        result.AppendMessage(f"{prefix}{name}: ...")
        return

    # Enum: lldb represents Rust enums with synthetic children.
    # The interesting child is the active variant.
    if num_children == 1 and not value:
        child = val.GetChildAtIndex(0)
        child_name = child.GetName() or ""
        # For enums, the single child is the active variant
        if child_name != name:
            result.AppendMessage(f"{prefix}{name}: {child_name}")
            _print_value(result, child, indent + 2, max_depth - 1)
            return

    # Struct/tuple: print name and recurse into children
    if num_children > 0:
        if name and name != "value":
            result.AppendMessage(f"{prefix}{name}:")
        for i in range(min(num_children, 32)):
            child = val.GetChildAtIndex(i)
            _print_value(result, child, indent + 2, max_depth - 1)
        if num_children > 32:
            result.AppendMessage(f"{prefix}  ... ({num_children - 32} more)")
        return

    # Fallback
    display = summary or value or "(empty)"
    result.AppendMessage(f"{prefix}{name} = {display}")


def _print_sbvalue(result, val, indent=4, max_depth=8, _depth=0):
    """Print an SBValue using lldb's synthetic formatters (from rust-lldb).

    The Rust lldb scripts register synthetic providers that transform the raw
    $variants$/$variant$N representation into clean enum variants, and provide
    summary strings for String, Vec, etc.  We access the *synthetic* children
    (GetChildAtIndex with use_synthetic=True, the default) so those kick in.
    """
    prefix = " " * indent
    name = val.GetName() or ""

    # Summary: synthetic formatters produce these for String, Vec, Rc, etc.
    summary = val.GetSummary()
    scalar = val.GetValue()

    if summary:
        result.AppendMessage(f"{prefix}{name} = {summary}")
        return
    if scalar is not None and val.GetNumChildren() == 0:
        result.AppendMessage(f"{prefix}{name} = {scalar}")
        return
    if _depth >= max_depth:
        result.AppendMessage(f"{prefix}{name} = ...")
        return

    num_children = val.GetNumChildren()
    if num_children > 0:
        if name:
            result.AppendMessage(f"{prefix}{name}:")
        for i in range(min(num_children, 32)):
            child = val.GetChildAtIndex(i)
            _print_sbvalue(result, child, indent + 2, max_depth, _depth + 1)
        if num_children > 32:
            result.AppendMessage(f"{prefix}  ... ({num_children - 32} more)")
    else:
        display = summary or scalar or "(opaque)"
        result.AppendMessage(f"{prefix}{name} = {display}")


def _dump_hex(process, result, addr, size):
    """Dump memory as hex + ASCII + u64s."""
    data = read_bytes(process, addr, size)
    if data:
        for off in range(0, len(data), 8):
            chunk = data[off:off + 8]
            hex_str = " ".join(f"{b:02x}" for b in chunk)
            ascii_str = "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)
            result.AppendMessage(f"    +{off:4d}: {hex_str:<24s}  {ascii_str}")

        if size >= 8:
            result.AppendMessage(f"  as u64s:")
            for off in range(0, size, 8):
                val = read_u64(process, addr + off)
                if val is not None:
                    result.AppendMessage(f"    +{off:4d}: 0x{val:016x}  ({val})")


def cmd_slotmap_dump(debugger, command, result, internal_dict):
    """Dump all occupied slots. Usage: slotmap_dump <map> --slot-size N [--max M]"""
    args = command.split()
    if len(args) < 1:
        result.AppendMessage("Usage: slotmap_dump <slotmap_var> --slot-size N [--max M]")
        return

    sm_name = args[0]
    slot_size = None
    max_display = 20

    i = 1
    while i < len(args):
        if args[i] == "--slot-size" and i + 1 < len(args):
            slot_size = int(args[i + 1])
            i += 2
        elif args[i] == "--max" and i + 1 < len(args):
            max_display = int(args[i + 1])
            i += 2
        else:
            result.AppendMessage(f"Unknown argument: {args[i]}")
            return

    if slot_size is None:
        result.AppendMessage("Error: --slot-size is required")
        return

    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()

    base_ptr, vec_len, err = get_slotmap_info(frame, sm_name)
    if err:
        result.AppendMessage(f"Error: {err}")
        return

    num_elems = eval_unsigned(frame, f"{sm_name}.num_elems")
    version_offset = detect_version_offset(process, base_ptr, slot_size)

    result.AppendMessage(f"SlotMap '{sm_name}': {num_elems} elements, {vec_len} slots allocated")
    result.AppendMessage(f"  slot_size={slot_size}, version_offset={version_offset}")
    result.AppendMessage("")

    displayed = 0
    for idx in range(vec_len):
        if displayed >= max_display:
            result.AppendMessage(f"  ... ({vec_len - idx} more slots)")
            break

        slot_addr = base_ptr + idx * slot_size
        version = read_u32(process, slot_addr + version_offset)
        if version is None:
            continue

        occupied = version % 2 == 1
        if not occupied:
            continue

        result.AppendMessage(f"  [{idx}] version={version}  addr=0x{slot_addr:x}")
        displayed += 1


def cmd_slotmap_detect(debugger, command, result, internal_dict):
    """Detect slot size by examining memory patterns. Usage: slotmap_detect <map>"""
    args = command.split()
    if len(args) < 1:
        result.AppendMessage("Usage: slotmap_detect <slotmap_var>")
        return

    sm_name = args[0]

    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()

    base_ptr, vec_len, err = get_slotmap_info(frame, sm_name)
    if err:
        result.AppendMessage(f"Error: {err}")
        return

    num_elems = eval_unsigned(frame, f"{sm_name}.num_elems")
    if num_elems is None or num_elems == 0:
        result.AppendMessage("SlotMap is empty, can't detect slot size. Insert at least one element.")
        return

    # Strategy: Slot 0 is vacant (version 0). Slot 1 is the first inserted element
    # (version 1, odd = occupied). We scan for the first occurrence of a u32 == 1
    # after the start of slot 1. Since slot 0 starts at base_ptr, we need to find
    # where slot 1's version field is.
    #
    # The version of slot 1 should be 1 (first occupation). We scan byte-by-byte
    # (aligned to 4) looking for value 1 as u32, after some minimum offset.

    # Read a generous chunk: we need at least 3 slots worth of data.
    # Max plausible slot size ~256 bytes, so 3*256=768. Use 1024 to be safe.
    chunk_size = min(1024, 256 * max(vec_len, 4))
    data = read_bytes(process, base_ptr, chunk_size)
    if data is None:
        result.AppendMessage("Could not read slot memory")
        return

    # Slot layout: [value union | version: u32 | padding]
    # Slot 0 is always vacant (version=0). Slot 1 is the first insert (version=1).
    # We find where version=1 sits for slot 1, and verify with slot 0's version=0.
    # Then validate: if we have >= 3 slots, slot 2's version should also be odd.
    #
    # The version offset within a slot is either:
    #   slot_size - 8  (8-byte aligned types: 4 bytes padding after version)
    #   slot_size - 4  (4-byte aligned types: no padding)

    def read_u32_at(off):
        if off + 4 > len(data):
            return None
        return int.from_bytes(data[off:off + 4], 'little')

    candidates = []
    for off in range(8, chunk_size - 3, 4):
        val = read_u32_at(off)
        if val != 1:
            continue

        # off = version_in_slot + slot_size (slot 1's version position)
        # Try version_in_slot at common offsets before this
        for v_off in range(4, off, 4):
            v0 = read_u32_at(v_off)
            if v0 != 0:
                continue
            slot_size = off - v_off
            if slot_size < 8 or slot_size % 4 != 0:
                continue

            # Structural constraint: version is right after the union,
            # so version_offset is either slot_size-8 (8-byte align) or
            # slot_size-4 (4-byte align). Nothing else is valid.
            version_in_slot = v_off  # offset of version within slot 0
            if version_in_slot != slot_size - 8 and version_in_slot != slot_size - 4:
                continue

            # If we have 3+ slots, verify slot 2 also has an odd version
            if vec_len >= 3 and num_elems >= 2:
                v2 = read_u32_at(v_off + 2 * slot_size)
                if v2 is None or v2 % 2 != 1:
                    continue

            candidates.append((slot_size, v_off))

    if not candidates:
        result.AppendMessage("Could not detect slot size. Please specify --slot-size manually.")
        return

    # Deduplicate: keep the smallest plausible slot_size per version_offset
    seen = {}
    for slot_size, v_off in candidates:
        if slot_size not in seen:
            seen[slot_size] = v_off

    result.AppendMessage(f"SlotMap '{sm_name}': {num_elems} elements, {vec_len} slots")
    result.AppendMessage(f"Candidate slot sizes:")
    for slot_size, v_off in sorted(seen.items()):
        result.AppendMessage(f"  slot_size = {slot_size}  (version at offset {v_off} within slot)")


# --- Registration ---

def __lldb_init_module(debugger, internal_dict):
    debugger.HandleCommand(
        'command script add -f slotmap_lldb.cmd_slotmap_get slotmap_get'
    )
    debugger.HandleCommand(
        'command script add -f slotmap_lldb.cmd_slotmap_dump slotmap_dump'
    )
    debugger.HandleCommand(
        'command script add -f slotmap_lldb.cmd_slotmap_detect slotmap_detect'
    )
    print("SlotMap LLDB commands loaded: slotmap_get, slotmap_dump, slotmap_detect")
