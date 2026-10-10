"""Run single functions of an original SC3U DLL in a 32-bit x86 emulator (Unicorn).

The DLL is mapped at its preferred image base, so the addresses cited in docs/ apply as is.
DllMain and the C runtime start-up never run: a check builds the objects it needs by hand and
calls the function under test directly.

Imports resolve to small stubs. A stub either runs a Python implementation (`STUBS` below) or,
when there is none, stops the run with the import's name, so a missing stub is never silent.
"""

import math
import struct

import pefile
from unicorn import (UC_ARCH_X86, UC_HOOK_CODE, UC_HOOK_MEM_INVALID, UC_MODE_32, Uc,
                     UcError)
from unicorn.x86_const import (UC_X86_REG_CS, UC_X86_REG_DS, UC_X86_REG_EAX,
                               UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EIP,
                               UC_X86_REG_ES, UC_X86_REG_ESP, UC_X86_REG_FP0,
                               UC_X86_REG_FPSW, UC_X86_REG_FS, UC_X86_REG_GDTR,
                               UC_X86_REG_GS, UC_X86_REG_SS)

PAGE = 0x1000
STACK_TOP, STACK_SIZE = 0x00F0_0000, 0x0010_0000   # 1 MiB below 15 MiB
HEAP_BASE, HEAP_SIZE = 0x0100_0000, 0x0400_0000    # 64 MiB bump heap
STUB_BASE, STUB_SIZE = 0x7F00_0000, 0x0004_0000    # one 64-byte stub per import
SCRATCH = 0x7F04_0000                              # doubles passed to and from stubs
RETURN = 0x7F04_1000                               # return address of every call: stops the run
TEB, GDT = 0x7FFD_E000, 0x7FFC_0000
STUB_STRIDE = 64
FAKE_BASE, FAKE_SIZE = 0x7F10_0000, 0x0001_0000    # methods of fake objects, 4 bytes each

# x87 control word of a Windows process: 53-bit precision, round to nearest, all exceptions
# masked (what the MSVC runtime sets up). Direct3D would switch to 24-bit.
WINDOWS_FPCW = 0x027F


def round_up(n, to=PAGE):
    return (n + to - 1) // to * to


def f80_to_float(mantissa, exponent):
    """An x87 register as Unicorn returns it, rounded to the nearest double."""
    sign = -1.0 if exponent & 0x8000 else 1.0
    e = exponent & 0x7FFF
    if e == 0 and mantissa == 0:
        return math.copysign(0.0, sign)
    if e == 0x7FFF:
        return sign * (math.inf if mantissa << 1 & (1 << 64) - 1 == 0 else math.nan)
    # Exact rational, then one rounding to double (Python rounds int/int correctly).
    shift = e - 16383 - 63
    if shift >= 0:
        return sign * float(mantissa << shift)
    return sign * (mantissa / (1 << -shift))


class EmuError(Exception):
    pass


class Stub:
    """A Python implementation of an import. `fn(emu)` reads stack arguments with `emu.arg`
    and returns through `emu.ret_*`. `x87_args` arguments come off the x87 stack first (MSVC's
    `_ftol` and `_CI*` helpers) and are read with `emu.x87_in`. A `double` result is pushed
    onto the x87 stack as ST0. `pops` is the stdcall argument size; cdecl is 0."""

    def __init__(self, fn, returns="int", x87_args=0, pops=0):
        self.fn, self.returns, self.x87_args, self.pops = fn, returns, x87_args, pops

    def code(self):
        """Machine code: x87 argument stores, the hook point, the result load, the return."""
        code = b"".join(b"\xDD\x1D" + struct.pack("<I", SCRATCH + 8 * i)    # fstp qword [in i]
                        for i in range(self.x87_args))
        code += b"\x90"                                                     # nop: hook point
        if self.returns == "double":
            code += b"\xDD\x05" + struct.pack("<I", SCRATCH + 0x40)          # fld qword [out]
        code += b"\xC2" + struct.pack("<H", self.pops) if self.pops else b"\xC3"
        return code

    def hook_offset(self):
        return 6 * self.x87_args


class NativeStub:
    """An import implemented as x86 code run by the emulator: helpers that rearrange the
    caller's stack frame and cannot be written as a call-and-return Python function, and hot
    ones (`abs`, `_ftol`, `isdigit`) where a Python callback per call would dominate the run."""

    fn = None

    def __init__(self, code):
        self._code = code

    def code(self):
        return self._code


class Emu:
    def __init__(self, path, fpcw=WINDOWS_FPCW):
        self.path = str(path)
        self.fpcw = fpcw
        self.uc = Uc(UC_ARCH_X86, UC_MODE_32)
        self.heap = HEAP_BASE
        self.sizes = {}          # allocation -> size, for realloc
        self.hooks = {}          # address -> [callback(emu)]
        self.stubs = {}          # stub address -> (dll, name)
        self.stub_error = None
        self.fakes = {}          # fake method address -> (object name, slot, fn)
        self.fake_next = FAKE_BASE
        self._map(STACK_TOP - STACK_SIZE, STACK_SIZE)
        self._map(HEAP_BASE, HEAP_SIZE)
        self._map(STUB_BASE, STUB_SIZE + 2 * PAGE)   # stubs, SCRATCH, RETURN
        self._map(FAKE_BASE, FAKE_SIZE)
        self._load(path)
        self.uc.mem_write(RETURN, b"\xF4")       # hlt; never executed, the run stops before
        self._fs()
        # Range-limited code hooks only: a hook on every instruction would be far too slow.
        # Stubs hook only their hook point (see `_load`).
        self.uc.hook_add(UC_HOOK_CODE, self._on_fake, begin=FAKE_BASE,
                         end=FAKE_BASE + FAKE_SIZE - 1)
        self.uc.hook_add(UC_HOOK_MEM_INVALID, self._on_bad_memory)

    # Loading -------------------------------------------------------------------------------

    def _map(self, base, size):
        self.uc.mem_map(base, round_up(size))

    def _load(self, path):
        pe = pefile.PE(self.path, fast_load=True)
        pe.parse_data_directories(directories=[
            pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"]])
        if pe.FILE_HEADER.Machine != 0x14C:
            raise EmuError(f"{path}: not a 32-bit x86 image")
        self.base = pe.OPTIONAL_HEADER.ImageBase
        self.size = round_up(pe.OPTIONAL_HEADER.SizeOfImage)
        self._map(self.base, self.size)
        self.uc.mem_write(self.base, pe.get_memory_mapped_image()[:self.size])
        self.sections = [(self.base + s.VirtualAddress, s.Misc_VirtualSize,
                          s.Name.rstrip(b"\0").decode(errors="replace")) for s in pe.sections]
        n = 0
        for entry in getattr(pe, "DIRECTORY_ENTRY_IMPORT", []):
            dll = entry.dll.decode().upper()
            for imp in entry.imports:
                name = imp.name.decode() if imp.name else f"#{imp.ordinal}"
                stub = STUB_BASE + n * STUB_STRIDE
                n += 1
                if n * STUB_STRIDE > STUB_SIZE:
                    raise EmuError("too many imports")
                self.stubs[stub] = (dll, name)
                self.uc.mem_write(imp.address, struct.pack("<I", stub))
                code = STUBS[name].code() if name in STUBS else b"\x90\xC3"
                assert len(code) <= STUB_STRIDE
                self.uc.mem_write(stub, code)
                if not isinstance(STUBS.get(name), NativeStub):
                    at = stub + (STUBS[name].hook_offset() if name in STUBS else 0)
                    self.uc.hook_add(UC_HOOK_CODE, self._on_stub, begin=at, end=at)

    def _fs(self):
        """A flat FS segment over a zeroed TEB, for MSVC's `fs:[0]` exception frames."""
        self._map(GDT, PAGE)
        self._map(TEB, 2 * PAGE)
        self.uc.mem_write(TEB, struct.pack("<III", 0xFFFFFFFF, STACK_TOP, STACK_TOP - STACK_SIZE))
        self.uc.mem_write(TEB + 0x18, struct.pack("<I", TEB))           # NT_TIB.Self

        def descriptor(base, limit, access, flags):
            return (limit & 0xFFFF) | (base & 0xFFFFFF) << 16 | access << 40 \
                | (limit >> 16 & 0xF) << 48 | flags << 52 | (base >> 24) << 56

        # Loading any segment register makes Unicorn re-read all of them, so the flat 32-bit
        # code and data segments must be real descriptors too.
        gdt = {1: descriptor(0, 0xFFFFF, 0x9B, 0xC),       # code, 4 GiB, 32-bit
               2: descriptor(0, 0xFFFFF, 0x93, 0xC),       # data, 4 GiB, 32-bit
               5: descriptor(TEB, 0xFFF, 0x93, 0x4)}       # fs: the TEB
        for i, d in gdt.items():
            self.uc.mem_write(GDT + 8 * i, struct.pack("<Q", d))
        self.uc.reg_write(UC_X86_REG_GDTR, (0, GDT, 8 * 16 - 1, 0))
        self.uc.reg_write(UC_X86_REG_CS, 1 << 3)
        for r in (UC_X86_REG_SS, UC_X86_REG_DS, UC_X86_REG_ES, UC_X86_REG_GS):
            self.uc.reg_write(r, 2 << 3)
        self.uc.reg_write(UC_X86_REG_FS, 5 << 3)

    # Memory --------------------------------------------------------------------------------

    def alloc(self, size):
        """Zeroed heap memory, 16-byte aligned. Never freed: a check runs a bounded workload."""
        at = self.heap
        self.heap = round_up(at + max(size, 1), 16)
        if self.heap > HEAP_BASE + HEAP_SIZE:
            raise EmuError("emulated heap exhausted")
        self.uc.mem_write(at, bytes(size))
        self.sizes[at] = size
        return at

    def read(self, at, n):
        return bytes(self.uc.mem_read(at, n))

    def u8(self, at):
        return self.read(at, 1)[0]

    def u32(self, at):
        return struct.unpack("<I", self.read(at, 4))[0]

    def f64(self, at):
        return struct.unpack("<d", self.read(at, 8))[0]

    def write(self, at, data):
        self.uc.mem_write(at, bytes(data))

    def w32(self, at, v):
        self.write(at, struct.pack("<I", v & 0xFFFFFFFF))

    def find_dwords(self, values, section=None):
        """Addresses where the dword sequence `values` (None = any) appears, dword-aligned."""
        hits = []
        for start, size, name in self.sections:
            if section and name != section:
                continue
            data = self.read(start, round_up(size, 4))
            words = struct.unpack(f"<{len(data) // 4}I", data)
            for i in range(len(words) - len(values) + 1):
                if all(v is None or words[i + j] == v for j, v in enumerate(values)):
                    hits.append(start + 4 * i)
        return hits

    def fake_object(self, name, methods, slots=128, size=16):
        """An object of `size` bytes whose vtable calls back into Python, standing in for an
        interface the code under test uses (a city, a DB segment, a record). `methods` maps a
        vtable offset to (argument count, fn); `fn(emu, this)` reads its arguments with
        `emu.arg` and returns EAX (None for 0). Methods are thiscall and pop their arguments.
        Any other slot stops the run, naming the object and the slot. The vtable has at least
        `slots` entries and always reaches past the highest method."""
        slots = max([slots] + [off // 4 + 64 for off in methods])
        vtable = self.alloc(4 * slots)
        for i in range(slots):
            at = self.fake_next
            self.fake_next += 4
            if self.fake_next > FAKE_BASE + FAKE_SIZE:
                raise EmuError("too many fake methods")
            nargs, fn = methods.get(4 * i, (0, None))
            self.write(at, b"\x90\xC2" + struct.pack("<H", 4 * nargs))     # nop; ret 4n
            self.fakes[at] = (name, 4 * i, fn)
            self.w32(vtable + 4 * i, at)
        obj = self.alloc(size)
        self.w32(obj, vtable)
        return obj

    # Calling -------------------------------------------------------------------------------

    def hook(self, address, callback):
        """Calls `callback(emu)` each time execution reaches `address`."""
        if address not in self.hooks:
            self.hooks[address] = []
            self.uc.hook_add(UC_HOOK_CODE, self._on_hooked, begin=address, end=address)
        self.hooks[address].append(callback)

    def once(self, address, callback):
        """Calls `callback(address)` the first time execution reaches `address`, then removes
        the hook, so watching a hot function costs one callback."""
        handle = []

        def hit(uc, at, size, _):
            if handle:
                callback(at)
                uc.hook_del(handle.pop())
        handle.append(self.uc.hook_add(UC_HOOK_CODE, hit, begin=address, end=address))

    def arg(self, i):
        """The i-th 32-bit stack argument at a function's entry (after the return address)."""
        return self.u32(self.reg(UC_X86_REG_ESP) + 4 + 4 * i)

    def reg(self, r):
        return self.uc.reg_read(r)

    def st0(self):
        top = self.reg(UC_X86_REG_FPSW) >> 11 & 7
        return f80_to_float(*self.uc.reg_read(UC_X86_REG_FP0 + top))

    def _reset_fpu(self):
        # fninit; fldcw [SCRATCH + 0x80]
        self.w32(SCRATCH + 0x80, self.fpcw)
        code = b"\xDB\xE3\xD9\x2D" + struct.pack("<I", SCRATCH + 0x80)
        at = SCRATCH + 0x100
        self.write(at, code)
        self.uc.emu_start(at, at + len(code))

    def call(self, address, args=(), this=None, returns="int"):
        """Calls `address` with `args` pushed right to left (ints as dwords, floats as doubles)
        and `this` in ECX. Works for thiscall, stdcall and cdecl alike: the stack is reset
        after each call. Returns EAX, or ST0 as a float for `returns="double"`."""
        self._reset_fpu()
        block = b"".join(struct.pack("<d", a) if isinstance(a, float)
                         else struct.pack("<I", a & 0xFFFFFFFF) for a in args)
        esp = STACK_TOP - 0x100 - len(block)
        self.write(esp, struct.pack("<I", RETURN) + block)
        self.uc.reg_write(UC_X86_REG_ESP, esp)
        self.uc.reg_write(UC_X86_REG_ECX, this or 0)
        self.stub_error = None
        try:
            self.uc.emu_start(address, RETURN)
        except UcError as e:
            if self.stub_error:
                raise EmuError(self.stub_error) from None
            raise EmuError(f"{e} at {self.reg(UC_X86_REG_EIP):#010x}") from None
        if self.stub_error:
            raise EmuError(self.stub_error)
        return self.st0() if returns == "double" else self.reg(UC_X86_REG_EAX)

    # Hooks ---------------------------------------------------------------------------------

    def _on_hooked(self, uc, address, size, _):
        for cb in self.hooks.get(address, ()):
            cb(self)

    def _on_stub(self, uc, address, size, _):
        stub_at = address - (address - STUB_BASE) % STUB_STRIDE
        if stub_at not in self.stubs:
            return
        dll, name = self.stubs[stub_at]
        stub = STUBS.get(name)
        if isinstance(stub, NativeStub):
            return
        if address != stub_at + (stub.hook_offset() if stub else 0):
            return
        caller = self.u32(self.reg(UC_X86_REG_ESP))
        if stub is None:
            self.stub_error = (f"unimplemented import {dll}!{name} called from {caller:#010x}; "
                               f"add a stub to STUBS in tools/diffcheck/emu.py")
            uc.emu_stop()
            return
        try:
            stub.fn(self)
        except Exception as e:  # noqa: BLE001 - surfaced as an EmuError
            self.stub_error = f"{dll}!{name} called from {caller:#010x}: {e}"
            uc.emu_stop()

    def _on_fake(self, uc, address, size, _):
        if address not in self.fakes:
            return
        name, slot, fn = self.fakes[address]
        caller = self.u32(self.reg(UC_X86_REG_ESP))
        if fn is None:
            self.stub_error = (f"{name} vtable slot {slot:#x} called from {caller:#010x}, "
                               f"which the fake does not implement")
            uc.emu_stop()
            return
        try:
            self.ret_int(fn(self, self.reg(UC_X86_REG_ECX)) or 0)
        except Exception as e:  # noqa: BLE001 - surfaced as an EmuError
            self.stub_error = f"{name} slot {slot:#x} called from {caller:#010x}: {e}"
            uc.emu_stop()

    def _on_bad_memory(self, uc, access, address, size, value, _):
        self.stub_error = (f"bad memory access at {address:#010x} (size {size}) from "
                           f"{self.reg(UC_X86_REG_EIP):#010x}")
        return False

    # Helpers for stubs ---------------------------------------------------------------------

    def ret_int(self, v):
        self.uc.reg_write(UC_X86_REG_EAX, v & 0xFFFFFFFF)

    def ret_int64(self, v):
        self.uc.reg_write(UC_X86_REG_EAX, v & 0xFFFFFFFF)
        self.uc.reg_write(UC_X86_REG_EDX, v >> 32 & 0xFFFFFFFF)

    def ret_double(self, v):
        self.write(SCRATCH + 0x40, struct.pack("<d", v))

    def x87_in(self, i):
        """The i-th x87 argument of an 'x87' stub (0 = what was ST0)."""
        return self.f64(SCRATCH + 8 * i)

    def stack_double(self, i):
        """A double stack argument starting at dword i."""
        sp = self.reg(UC_X86_REG_ESP) + 4 + 4 * i
        return self.f64(sp)


def _new(e):
    e.ret_int(e.alloc(e.arg(0)))


def _calloc(e):
    e.ret_int(e.alloc(e.arg(0) * e.arg(1)))


def _realloc(e):
    old, n = e.arg(0), e.arg(1)
    at = e.alloc(n)
    if old:
        e.write(at, e.read(old, min(n, e.sizes[old])))
    e.ret_int(at)


def _nothing(e):
    e.ret_int(0)


def _first_arg(e):
    e.ret_int(e.arg(0))


def _interlocked(op):
    """`Interlocked*` (stdcall): the new value for Increment/Decrement, the old for Exchange."""
    def run(e):
        at = e.arg(0)
        old = e.u32(at)
        new = {"inc": old + 1, "dec": old - 1, "xchg": e.arg(1) if op == "xchg" else 0}[op]
        e.w32(at, new)
        e.ret_int(old if op == "xchg" else new)
    return run


def _memset(e):
    dst, v, n = e.arg(0), e.arg(1), e.arg(2)
    e.write(dst, bytes([v & 0xFF]) * n)
    e.ret_int(dst)


def _memcpy(e):
    dst, src, n = e.arg(0), e.arg(1), e.arg(2)
    e.write(dst, e.read(src, n))
    e.ret_int(dst)


# MSVC's _ftol: pops ST0 and truncates it toward zero into EDX:EAX, by switching the x87
# rounding mode to chop around a fistp.
_FTOL = bytes.fromhex(
    "55"                # push ebp
    "8bec"              # mov ebp, esp
    "83c4f4"            # add esp, -12
    "d97dfe"            # fnstcw [ebp-2]
    "668b45fe"          # mov ax, [ebp-2]
    "80cc0c"            # or ah, 0Ch          ; round toward zero
    "668945fc"          # mov [ebp-4], ax
    "d96dfc"            # fldcw [ebp-4]
    "df7df4"            # fistp qword [ebp-12]
    "d96dfe"            # fldcw [ebp-2]
    "8b45f4"            # mov eax, [ebp-12]
    "8b55f8"            # mov edx, [ebp-8]
    "c9"                # leave
    "c3")               # ret

_ABS = bytes.fromhex(
    "8b442404"          # mov eax, [esp+4]
    "99"                # cdq
    "31d0"              # xor eax, edx
    "29d0"              # sub eax, edx
    "c3")               # ret

# "C" locale. The callers pass a sign-extended char, so bytes from 0x80 arrive negative;
# MSVCRT would index its table out of range for them, taken here as "not a digit".
_ISDIGIT = bytes.fromhex(
    "8b442404"          # mov eax, [esp+4]
    "83e830"            # sub eax, '0'
    "83f809"            # cmp eax, 9
    "7706"              # ja not_digit         ; also every negative c
    "b804000000"        # mov eax, 4           ; _DIGIT
    "c3"                # ret
    "31c0"              # not_digit: xor eax, eax
    "c3")               # ret


def _math1(f):
    def run(e):
        e.ret_double(f(e.stack_double(0)))
    return run


def _ci1(f):
    def run(e):
        e.ret_double(f(e.x87_in(0)))
    return run


def _cipow(e):
    # _CIpow: x in ST1, y in ST0; stored ST0 first.
    y, x = e.x87_in(0), e.x87_in(1)
    e.ret_double(math.pow(x, y))


def _strlen(e):
    at, n = e.arg(0), 0
    while e.u8(at + n):
        n += 1
    e.ret_int(n)


def _time(e):
    e.ret_int(0)


def _cstr(e, at, stop=b""):
    """The bytes from `at` up to the first NUL or byte of `stop`, read 64 bytes at a time: one
    memory read per byte would dominate a check that tokenizes whole files."""
    out = b""
    while True:
        try:
            chunk = e.read(at + len(out), 64)
        except UcError:
            chunk = e.read(at + len(out), 1)
        for i, b in enumerate(chunk):
            if b == 0 or b in stop:
                return out + chunk[:i]
        out += chunk


def _strcpy(e):
    dst, src = e.arg(0), e.arg(1)
    e.write(dst, _cstr(e, src) + b"\0")
    e.ret_int(dst)


def _strtok(e):
    # MSVCRT strtok: one saved position per process, kept on the emulator.
    at = e.arg(0) or getattr(e, "strtok_next", 0)
    delims = _cstr(e, e.arg(1))
    if at:
        rest = bytes(range(1, 256)).translate(None, delims)     # every byte but NUL and delims
        at += len(_cstr(e, at, rest))
    token = _cstr(e, at, delims) if at else b""
    if not token:
        e.strtok_next = at
        e.ret_int(0)
        return
    end = at + len(token)
    if e.u8(end):
        e.write(end, b"\0")
        end += 1
    e.strtok_next = end
    e.ret_int(at)


def _atoi(e):
    # MSVCRT atoi (atox.c): no overflow check, the total wraps at 32 bits.
    text = _cstr(e, e.arg(0)).lstrip(b" \t\n\v\f\r")
    sign = text[:1]
    if sign in (b"+", b"-"):
        text = text[1:]
    total = 0
    for b in text:
        if not 0x30 <= b <= 0x39:
            break
        total = (10 * total + b - 0x30) & 0xFFFFFFFF
    e.ret_int(-total if sign == b"-" else total)


# MSVC's _EH_prolog: EAX holds the frame's handler. Pushes the C++ exception registration
# (state -1, handler, previous fs:[0]), links it into fs:[0] and sets up EBP the way the
# function's own prologue would have.
_EH_PROLOG = bytes.fromhex(
    "6aff"              # push -1
    "50"                # push eax
    "64a100000000"      # mov eax, fs:[0]
    "50"                # push eax
    "64892500000000"    # mov fs:[0], esp
    "8b44240c"          # mov eax, [esp+0Ch]   ; return address
    "896c240c"          # mov [esp+0Ch], ebp
    "8d6c240c"          # lea ebp, [esp+0Ch]
    "50"                # push eax
    "c3")               # ret


STUBS = {
    # C++ exception frames.
    "_EH_prolog": NativeStub(_EH_PROLOG),
    # Heap. operator new / new[] / delete / delete[] (MSVC mangled names) and the C heap.
    "??2@YAPAXI@Z": Stub(_new),
    "??_U@YAPAXI@Z": Stub(_new),
    "??3@YAXPAX@Z": Stub(_nothing),
    "??_V@YAXPAX@Z": Stub(_nothing),
    "malloc": Stub(_new),
    "calloc": Stub(_calloc),
    "realloc": Stub(_realloc),
    "free": Stub(_nothing),
    "memset": Stub(_memset),
    "memcpy": Stub(_memcpy),
    "memmove": Stub(_memcpy),
    "strlen": Stub(_strlen),
    "strcpy": Stub(_strcpy),
    "strtok": Stub(_strtok),
    "isdigit": NativeStub(_ISDIGIT),
    "atoi": Stub(_atoi),
    "abs": NativeStub(_ABS),
    # Floating point. Python's math matches a correctly rounded x87 result in 53-bit mode for
    # sqrt; sin/cos/pow go through the host libm, like the Rust port does.
    "_ftol": NativeStub(_FTOL),
    "sqrt": Stub(_math1(math.sqrt), returns="double"),
    "sin": Stub(_math1(math.sin), returns="double"),
    "cos": Stub(_math1(math.cos), returns="double"),
    "acos": Stub(_math1(math.acos), returns="double"),
    "floor": Stub(_math1(math.floor), returns="double"),
    "ceil": Stub(_math1(math.ceil), returns="double"),
    "_CIsqrt": Stub(_ci1(math.sqrt), returns="double", x87_args=1),
    "_CIsin": Stub(_ci1(math.sin), returns="double", x87_args=1),
    "_CIcos": Stub(_ci1(math.cos), returns="double", x87_args=1),
    "_CIpow": Stub(_cipow, returns="double", x87_args=2),
    # Clock. Only reached with the "seed from the clock" seed 0xFFFFFFFF, which checks avoid.
    "InterlockedIncrement": Stub(_interlocked("inc"), pops=4),
    "InterlockedDecrement": Stub(_interlocked("dec"), pops=4),
    "InterlockedExchange": Stub(_interlocked("xchg"), pops=8),
    "timeGetTime": Stub(_time),
    # Exit handlers never run: registering one just hands it back.
    "__dllonexit": Stub(_first_arg),
    # One thread: locks always succeed.
    "EnterCriticalSection": Stub(_nothing, pops=4),
    "LeaveCriticalSection": Stub(_nothing, pops=4),
    "GetTickCount": Stub(_time),
}
