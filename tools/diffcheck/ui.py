"""The main UI layout check: three SIMUI.DLL functions against `sc3k_ui::main_ui`.

- `cSC3MainUIMgr::place_windows`: where the menu panel, navigator, RCI meter and date bar go,
  for many screen sizes and with each window missing in turn. The root window and the four
  windows are fakes (`Emu.fake_object`) that report their sizes and record the `Move` and
  `SetArea` calls the original makes.
- `cSC3WinMenuBtnMain::get_menu_btn_info_main`: each main button's area, images and submenu
  offset, for every button ID around the nine and many screen heights.
- `cSC3WinDateCashTitle::get_layout_info`: the bar's art group per screen width.

`sc3k-dump diffref ui` answers the same queries from the port. A query matches when both
give the same answer, including "none" where the original refuses.
"""

import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

FUNCTIONS = ["place_windows", "get_menu_btn_info_main", "get_layout_info"]
SCREENS = [(640, 480), (800, 600), (1024, 768), (1152, 864), (1280, 960), (1280, 1024),
           (1600, 1200), (1920, 1080), (800, 601), (700, 500), (2560, 1440)]
WINDOWS = ["panel", "nav", "rci", "bar"]


@dataclass
class UiStats:
    queries: int = 0
    exact: int = 0
    first: str = ""

    def rate(self):
        return self.exact / self.queries if self.queries else 1.0


@dataclass
class Query:
    function: str
    text: str                   # the diffref script line
    original: str = ""
    port: str = ""


def sizes_for(w, h):
    """The windows' sizes before placement as the port's art gives them, (w, h) each."""
    panel = (96, 334) if h == 480 else (96, 417)
    bar = (440, 64) if w == 640 else (600, 56) if w == 800 else (824, 64)
    return {"panel": panel, "nav": (160, 164), "rci": (41, 88), "bar": bar}


def queries():
    out = []
    for w, h in SCREENS:
        full = sizes_for(w, h)
        # Every window, then each one missing.
        for missing in [None] + WINDOWS:
            s = {k: (v if k != missing else (0, 0)) for k, v in full.items()}
            nums = " ".join(f"{s[k][0]} {s[k][1]}" for k in WINDOWS)
            out.append(Query("place_windows", f"place {w} {h} {nums}"))
    for h in sorted({h for _, h in SCREENS} | {0, 479, 599}):
        for id in range(0x1000, 0x100B):
            out.append(Query("get_menu_btn_info_main", f"button {id} {h}"))
    for w in sorted({w for w, _ in SCREENS} | {0, 639, 641, 799, 801}):
        out.append(Query("get_layout_info", f"bar {w}"))
    return out


def port(exe, qs):
    with tempfile.TemporaryDirectory() as d:
        src, out = Path(d) / "script.txt", Path(d) / "out.txt"
        src.write_text("".join(q.text + "\n" for q in qs))
        subprocess.run([str(exe), "diffref", "ui", str(src), str(out)], check=True)
        lines = out.read_text().splitlines()
    if len(lines) != len(qs):
        raise RuntimeError(f"diffref ui answered {len(lines)} of {len(qs)} queries")
    for q, line in zip(qs, lines):
        q.port = line.strip()


def signed(v):
    return v - (1 << 32) if v >> 31 else v


class Window:
    """A child window: reports its size and position, records Move and SetArea."""

    def __init__(self, emu, name):
        self.x = self.y = self.w = self.h = 0
        self.placed = None
        self.obj = emu.fake_object(name, {
            0x90: (0, lambda e, this: self.w),
            0x94: (0, lambda e, this: self.h),
            0x98: (0, lambda e, this: self.x),
            0x9C: (0, lambda e, this: self.y),
            0xCC: (2, self.move),
            0xC8: (4, self.set_area),
        })

    def reset(self, w, h):
        self.x = self.y = 0
        self.w, self.h = w, h
        self.placed = None

    def move(self, e, this):
        self.x, self.y = signed(e.arg(0)), signed(e.arg(1))
        self.placed = (self.x, self.y, self.w, self.h)

    def set_area(self, e, this):
        l, t, r, b = (signed(e.arg(i)) for i in range(4))
        self.x, self.y, self.w, self.h = l, t, r - l, b - t
        self.placed = (l, t, r - l, b - t)


class Original:
    """The three functions on one emulator. The fakes are made once and their answers set per
    query: the emulator has room for about 128 fake objects."""

    def __init__(self, emu, target):
        self.emu, u = emu, target.ui
        self.u = u
        self.ids = {"panel": u["id_panel"], "nav": u["id_nav"], "rci": u["id_rci"],
                    "bar": u["id_bar"]}
        self.windows = {k: Window(emu, k) for k in WINDOWS}
        self.present = {}
        self.area = emu.alloc(16)
        self.root = emu.fake_object("root window", {
            u["get_area"]: (0, lambda e, this: self.area),
            u["get_child"]: (1, lambda e, this: self.present.get(e.arg(0), 0)),
        })
        wm = emu.fake_object("cRZWinManager", {u["wm_is_window"]: (1, lambda e, this: 1)})
        emu.w32(u["wm_global"], wm)
        self.mgr = emu.alloc(0x40)
        emu.w32(self.mgr + u["mgr_root"], self.root)

        self.height = self.width = self.id = 0
        main = emu.fake_object("main window", {u["win_height"]: (0, lambda e, this: self.height)})
        self.btn = emu.fake_object("cSC3WinMenuBtnMain", {
            u["btn_parent"]: (0, lambda e, this: main),
            u["btn_id"]: (0, lambda e, this: self.id),
        })
        screen = emu.fake_object("screen window", {u["win_width"]: (0, lambda e, this: self.width)})
        parent = emu.fake_object("parent", {u["dct_parent_up"]: (0, lambda e, this: screen)})
        self.dct = emu.fake_object("cSC3WinDateCashTitle",
                                   {u["dct_parent"]: (0, lambda e, this: parent)})
        self.info = emu.alloc(0x80)

    def place(self, q):
        emu = self.emu
        words = [int(x) for x in q.text.split()[1:]]
        w, h = words[0], words[1]
        self.present = {}
        for i, k in enumerate(WINDOWS):
            size = (words[2 + 2 * i], words[3 + 2 * i])
            self.windows[k].reset(*size)
            if size != (0, 0):
                self.present[self.ids[k]] = self.windows[k].obj
        for i, v in enumerate((0, 0, w, h)):
            emu.w32(self.area + 4 * i, v)
        if not emu.call(self.u["place_windows"], [], this=self.mgr) & 0xFF:
            return "refused"
        parts = []
        for k in WINDOWS:
            p = self.windows[k].placed if self.ids[k] in self.present else None
            parts.append(" ".join(str(v) for v in p) if p else "- - - -")
        return " ".join(parts)

    def button(self, q):
        emu = self.emu
        self.id, self.height = (int(x) for x in q.text.split()[1:])
        emu.write(self.info, bytes(0x80))
        if not emu.call(self.u["get_menu_btn_info_main"], [self.info], this=self.btn) & 0xFF:
            return "none"
        f = [emu.u32(self.info + 4 * i) for i in range(0x1B)]
        default = f[2]

        def entry(k):
            g, inst = f[3 + 3 * k], f[4 + 3 * k]
            return (g or default) if inst else 0, inst

        hover_group, hover = entry(1)
        _, hover_open = entry(5)
        sub_group, sub = entry(4)
        # The offset only places the submenu image; without one it is never used.
        dx, dy = (signed(f[0x19]), signed(f[0x1A])) if sub else (0, 0)
        x, y, w, h = signed(f[0x17]), signed(f[0x18]), f[0x15], f[0x16]
        return f"{x} {y} {w} {h} {hover_group} {hover} {hover_open} {sub_group} {sub} {dx} {dy}"

    def bar(self, q):
        emu = self.emu
        self.width = int(q.text.split()[1])
        emu.write(self.info, bytes(0x80))
        if not emu.call(self.u["get_layout_info"], [self.info], this=self.dct) & 0xFF:
            return "none"
        return str(emu.u32(self.info))


def check_ui(make_emu, target, exe):
    """{function: UiStats}."""
    qs = queries()
    port(exe, qs)
    stats = {f: UiStats() for f in FUNCTIONS}
    o = Original(make_emu(), target)
    run = {"place_windows": o.place, "get_menu_btn_info_main": o.button,
           "get_layout_info": o.bar}
    for q in qs:
        q.original = run[q.function](q)
        s = stats[q.function]
        s.queries += 1
        if q.original == q.port:
            s.exact += 1
        elif not s.first:
            s.first = f"`{q.text}`: original `{q.original}`, port `{q.port}`"
    return stats
