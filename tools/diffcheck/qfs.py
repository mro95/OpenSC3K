"""The QFS check: cRZFastCompression3 in SIMBABLD.DLL against `sc3k_formats::qfs`.

Three sources of streams, each its own row:
- install: QFS streams from the containers under $SC3K_DATA (`sc3k-dump diffref qfs-samples`);
- round trip: data compressed by the original `CompressData`, so every opcode the game's own
  encoder emits is covered;
- edge cases: streams assembled here, with every opcode form, overlapping copies, the largest
  offsets and lengths, and the header variants (compressed-size field, 4-byte sizes).
A stream matches when both decoders produce the same bytes, or both reject it.
"""

import random
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

import rust
from emu import HEAP_BASE

SOURCES = ["install", "round trip", "edge cases"]


@dataclass
class QfsStats:
    streams: int = 0
    exact: int = 0
    bytes: int = 0
    bytes_equal: int = 0
    first: str = ""

    def rate(self):
        return self.exact / self.streams if self.streams else 1.0

    def byte_rate(self):
        return self.bytes_equal / self.bytes if self.bytes else 1.0


# Edge-case streams --------------------------------------------------------------------------

def _op(form, literal, copy, offset):
    """One control code: `literal` (0..3) bytes, then `copy` bytes from `offset` back."""
    o = offset - 1
    if form == 2:       # 00-7F: copy 3..10, offset 1..1024
        return bytes([(o >> 8) << 5 | (copy - 3) << 2 | literal, o & 0xFF])
    if form == 3:       # 80-BF: copy 4..67, offset 1..16384
        return bytes([0x80 | (copy - 4), literal << 6 | o >> 8, o & 0xFF])
    c = copy - 5        # C0-DF: copy 5..1028, offset 1..131072
    return bytes([0xC0 | (o >> 16) << 4 | (c >> 8) << 2 | literal, o >> 8 & 0xFF, o & 0xFF,
                  c & 0xFF])


FORMS = {2: (3, 10, 1024), 3: (4, 67, 16384), 4: (5, 1028, 131072)}


class Assembler:
    """Builds a stream and the output it must decode to."""

    def __init__(self):
        self.code, self.out, self.pending = bytearray(), bytearray(), b""

    def literal(self, data):
        self.pending += data
        self.out += data

    def _flush(self, keep):
        """Writes pending literals as E0-FB blocks (4..112, multiples of 4), leaving `keep`."""
        lit = self.pending[:len(self.pending) - keep] if keep else self.pending
        while lit:
            k = min(112, len(lit) // 4 * 4)
            self.code += bytes([0xE0 | (k - 4) >> 2]) + lit[:k]
            lit = lit[k:]
        tail = self.pending[len(self.pending) - keep:] if keep else b""
        self.pending = b""
        return tail

    def copy(self, form, copy, offset):
        keep = len(self.pending) % 4
        tail = self._flush(keep)
        self.code += _op(form, keep, copy, offset) + tail
        for _ in range(copy):
            self.out.append(self.out[-offset])

    def stream(self, flags=0x10):
        keep = len(self.pending) % 4
        tail = self._flush(keep)
        body = bytes(self.code) + bytes([0xFC | keep]) + tail
        width = 4 if flags & 0x80 else 3
        head_len = 2 + width * (2 if flags & 1 else 1)
        size = lambda n: n.to_bytes(width, "big")  # noqa: E731
        head = bytes([flags, 0xFB])
        if flags & 1:
            head += size(head_len + len(body))
        return head + size(len(self.out)) + body, bytes(self.out)


def edge_streams(r):
    """[(label, stream)]: random op mixes per form and header, plus the extremes."""
    out = []
    for flags in (0x10, 0x11, 0x90, 0x91):
        for n in range(40):
            a = Assembler()
            a.literal(bytes(r.randrange(256) for _ in range(r.randrange(1, 40))))
            for _ in range(r.randrange(1, 60)):
                if r.random() < 0.3:
                    a.literal(bytes(r.randrange(256) for _ in range(r.randrange(0, 130))))
                form = r.choice([2, 3, 4])
                lo, hi, far = FORMS[form]
                a.copy(form, r.randint(lo, hi), r.randint(1, min(far, len(a.out))))
            out.append((f"edge flags {flags:#04x} #{n}", a.stream(flags)[0]))
    # Extremes: overlapping copies of one byte, and the farthest offset of each form.
    a = Assembler()
    a.literal(b"x")
    for form, (lo, hi, _) in FORMS.items():
        a.copy(form, lo, 1)
        a.copy(form, hi, 1)
    out.append(("edge overlapping copies", a.stream()[0]))
    a = Assembler()
    a.literal(bytes(r.randrange(256) for _ in range(140_000)))
    for form, (lo, hi, far) in FORMS.items():
        a.copy(form, hi, far)
        a.copy(form, lo, far)
    out.append(("edge farthest offsets", a.stream()[0]))
    a = Assembler()
    out.append(("edge empty", a.stream()[0]))
    a.literal(b"abc")
    out.append(("edge 3 literals only", a.stream()[0]))
    return out


def roundtrip_inputs(r):
    """[(label, data)] for the original compressor."""
    words = [b"the ", b"city ", b"mayor ", b"road ", b"power ", b"water ", b"zone "]
    text = b"".join(r.choice(words) for _ in range(12_000))
    noisy = bytearray(r.randrange(256) for _ in range(60_000))
    for _ in range(3_000):
        at, n = r.randrange(len(noisy) - 300), r.randrange(3, 300)
        src = r.randrange(max(1, at - 140_000), at) if at else 0
        noisy[at:at + n] = noisy[src:src + n]
    return [
        ("1 zero byte", bytes(1)),
        ("100000 zero bytes", bytes(100_000)),
        ("period 3", b"abc" * 20_000),
        ("4000 random bytes", bytes(r.randrange(256) for _ in range(4_000))),
        ("text", text),
        ("random with repeats", bytes(noisy)),
        ("RGB565 gradient", b"".join((x * 33 + y * 7 & 0xFFFF).to_bytes(2, "little")
                                     for y in range(128) for x in range(256))),
        ("200000 bytes, far repeats", bytes(noisy) + bytes(noisy[::-1]) + bytes(noisy)
         + bytes(20_000)),
    ]


# The original -------------------------------------------------------------------------------

class Original:
    """cRZFastCompression3 on a hand-built object in a fresh emulator, renewed as the bump heap
    fills."""

    def __init__(self, make_emu, target):
        self.make_emu, self.q = make_emu, target.qfs
        self._fresh()

    def _fresh(self):
        self.emu = self.make_emu()
        q = self.q
        hits = self.emu.find_dwords([q["QueryInterface"], q["AddRef"], q["Release"]])
        if len(hits) != 1:
            raise RuntimeError(f"expected one cRZFastCompression3 vtable, found {len(hits)}")
        self.this = self.emu.alloc(q["size"])
        self.emu.w32(self.this, hits[0])
        self.emu.w32(self.this + 4, 1)              # reference count

    def _room(self, n):
        if self.emu.heap - HEAP_BASE + 4 * n > 0x0200_0000:
            self._fresh()

    def decompress(self, stream):
        """(True, bytes) or (False, None) when DecompressData returns false."""
        self._room(len(stream) + 3 * (1 << 20))
        e, q = self.emu, self.q
        src = e.alloc(len(stream) + 8)
        e.write(src, stream)
        n = e.call(q["GetLengthOfDecompressedData"], [src], this=self.this)
        self._room(n)
        dst, out_len = e.alloc(n + 16), e.alloc(4)
        e.w32(out_len, n)
        ok = e.call(q["DecompressData"], [src, len(stream), dst, out_len], this=self.this) & 0xFF
        if not ok:
            return False, None
        return True, e.read(dst, min(e.u32(out_len), n + 16))

    def compress(self, data):
        self._room(4 * len(data) + (1 << 20))
        e, q = self.emu, self.q
        cap = e.call(q["GetMaxLengthRequiredForCompressedData"], [len(data)], this=self.this)
        src, dst, out_len = e.alloc(len(data)), e.alloc(cap + 16), e.alloc(4)
        e.write(src, data)
        e.w32(out_len, cap)
        ok = e.call(q["CompressData"], [src, len(data), dst, out_len], this=self.this) & 0xFF
        return e.read(dst, e.u32(out_len)) if ok else None


# The check ----------------------------------------------------------------------------------

def install_streams(exe, root, limit):
    """[(label, stream)] from the install, or [] without one."""
    if not root:
        return []
    with tempfile.TemporaryDirectory() as d:
        subprocess.run([str(exe), "diffref", "qfs-samples", str(root), d, str(limit)],
                       check=True, stdout=subprocess.DEVNULL)
        index = {}
        for line in (Path(d) / "index.tsv").read_text().splitlines():
            name, file, tgi = line.split("\t")
            index[name] = f"{file} {tgi}"
        return [(index[p.stem], p.read_bytes()) for p in sorted(Path(d).glob("*.qfs"))]


def compare(stats, label, original, port):
    ok_o, want = original
    ok_p, got = port
    stats.streams += 1
    expected = len(want) if ok_o else len(got) if ok_p else 0
    stats.bytes += expected
    if ok_o and ok_p:
        stats.bytes_equal += sum(a == b for a, b in zip(want, got))
    if (ok_o, ok_p) == (False, False) or (ok_o and ok_p and want == got):
        stats.exact += 1
        return
    if stats.first:
        return
    if not ok_o:
        why = f"original rejects it, port gives {len(got)} bytes"
    elif not ok_p:
        why = f"original gives {len(want)} bytes, port rejects it: {got}"
    elif len(want) != len(got):
        why = f"original gives {len(want)} bytes, port {len(got)}"
    else:
        at = next(i for i, (a, b) in enumerate(zip(want, got)) if a != b)
        why = f"byte {at}: original {want[at]:#04x}, port {got[at]:#04x}"
    stats.first = f"{label}: {why}"


def check_qfs(make_emu, target, exe, root=None, limit=500, seed=0x5C3):
    """Per source: streams decoded identically and bytes equal."""
    r = random.Random(seed)
    orig = Original(make_emu, target)
    sources = {"install": install_streams(exe, root, limit), "round trip": [],
               "edge cases": edge_streams(r)}
    for label, data in roundtrip_inputs(r):
        packed = orig.compress(data)
        if packed is None:
            raise RuntimeError(f"cRZFastCompression3::CompressData refused {label}")
        sources["round trip"].append((f"round trip of {label}", packed))
    stats = {}
    for source, streams in sources.items():
        if not streams:
            continue
        s = stats[source] = QfsStats()
        port = rust.qfs(exe, [st for _, st in streams])
        for (label, stream), p in zip(streams, port):
            compare(s, label, orig.decompress(stream), p)
    return stats
