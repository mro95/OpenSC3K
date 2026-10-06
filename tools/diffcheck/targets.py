"""Addresses of the original functions under test, from docs/sim/random.md,
docs/sim/terrain-gen.md and tools/match/names. A check only uses what is listed here."""

from dataclasses import dataclass, field

# Order and numbering match `sc3k_sim::rng::Op`.
RNG_OPS = ["seed", "next_u32", "uniform", "range", "gaussian_fast", "range_min_of_two",
           "double", "double_range"]
# Argument kinds per op: "i" a 32-bit integer, "d" a double.
RNG_ARGS = {"seed": "i", "next_u32": "", "uniform": "i", "range": "ii", "gaussian_fast": "ii",
            "range_min_of_two": "ii", "double": "", "double_range": "dd"}
RNG_DOUBLE_RESULT = {"double", "double_range"}


@dataclass
class Target:
    dll: str
    # cRZRandom method entry points, by op name.
    rng: dict = field(default_factory=dict)
    # [start, end) of the cRZRandom code: a call from inside it is nested, not logged.
    rng_code: tuple = (0, 0)
    # Thunks that jump to an RNG method (address -> op name).
    rng_thunks: dict = field(default_factory=dict)
    # cSC3DirtGenerator. The vtable is found at run time from these slots.
    generator: dict = field(default_factory=dict)
    # Named stages of GenerateRandom, for saying where a trace diverged (address -> name).
    stages: dict = field(default_factory=dict)
    # cRZFastCompression3 (QFS). The vtable is found at run time from its first three slots.
    qfs: dict = field(default_factory=dict)
    # cSC3DirtBag: loading a saved terrain, object layout and the fakes' vtable slots.
    dirt_bag: dict = field(default_factory=dict)
    # The main UI's layout functions and the window slots their fakes answer.
    ui: dict = field(default_factory=dict)
    # The tiling rule readers of cSTTransitLayer, and the cRZFile slots their fake answers.
    tiling: dict = field(default_factory=dict)
    # Checks that run against this DLL (run.py subcommands).
    checks: tuple = ()


SIMDIRT = Target(
    dll="SIMDIRT.DLL",
    checks=("rng", "dirt", "ground"),
    rng={
        "seed": 0x1001BB50,
        "next_u32": 0x1001BB6B,
        "uniform": 0x1001BBA8,
        "range": 0x1001BC23,
        "gaussian_fast": 0x1001BC58,
        "range_min_of_two": 0x1001BCA4,
        "double": 0x1001BCD9,
        "double_range": 0x1001BD1B,
    },
    rng_code=(0x1001BB50, 0x1001BD31),
    rng_thunks={0x1001BC38: "gaussian_fast", 0x1001BC48: "range_min_of_two"},
    generator={
        # Vtable slots 0x0C..0x1C, no 2-slot header.
        "Init": 0x1001742C,
        "Shutdown": 0x100175A2,
        "SetDifficulty": 0x10017375,
        "IsReady": 0x10017412,
        "GenerateRandom": 0x10017C2D,
        # Object layout.
        "sea": 0x10,
        "altitude": 0x14,
        "water": 0x18,
        "flora": 0x1C,
        "salt": 0x20,
        "size": 0x40,
    },
    stages={
        0x10017C2D: "GenerateRandom",
        0x100177B7: "AltitudeSubdivision",
        0x10017AF6: "ChangeTerrainProfile",
        0x100181D6: "FindSaltWater",
        0x10018427: "CreateFlora",
        0x100185C8: "FixShores",
        0x100187DA: "CreateRivers",
        0x10018B57: "traceDepth",
        0x10018C30: "Bezier2D4Controls",
        0x10017A09: "blend of 2 values",
        0x10017A69: "blend of 4 values",
    },
    # Found from the vtable that holds the named Init(cISC3City*, cISC2Importer*) and Save;
    # layout from Init(cISC3City*) at 0x10003E50 (docs/formats/save.md).
    dirt_bag={
        "Init": 0x10004A00,                 # Init(cISC3City*, cIGZDBSegment*)
        "calculateAndSetVertexLight": 0x10007010,
        "vtable": 0x1002046C,
        "key": (0x206C6E7C, 0x21737DE5, 0),
        "size": 0x80,
        "ready": 0x24,                      # set by Init(cISC3City*); the accessors check it
        "altitude": 0x30,
        "light": 0x34,
        "water": 0x38,
        "sea": 0x3C,
        "cell_bits": (0x50, 0x54, 0x58, 0x5C),
        "city": 0x64,
        "lock": 0x100249F0,                 # static cRZCriticalSection
        "lock_updates": 0x1C,               # cSC3CityChangeSender slot
        "city_cells_x": 0xCC,               # cISC3City slots
        "city_cells_z": 0xD0,
        "city_version": 0x260,
    },
)


# cRZFastCompression3, named by tools/match from the Loki demo's symbols (sc3u_demo.x86).
SIMBABLD = Target(
    dll="SIMBABLD.DLL",
    checks=("qfs",),
    qfs={
        # Vtable: QueryInterface, AddRef, Release, ...
        "QueryInterface": 0x12059E3F,
        "AddRef": 0x12059E64,
        "Release": 0x12059E6B,
        "CompressData": 0x12059E86,
        "DecompressData": 0x12059EBE,
        "GetMaxLengthRequiredForCompressedData": 0x12059F0F,
        "GetLengthOfDecompressedData": 0x12059F30,
        "size": 0x40,
    },
    stages={
        0x12059E86: "cRZFastCompression3::CompressData",
        0x12059EBE: "cRZFastCompression3::DecompressData",
    },
)

# The main UI's layout (docs/ui/main-ui.md). Windows cIGZWin slots are the Loki ones minus 8
# from 0x80 on (GetChildWindowFromID 0x78, GetArea 0xAC, ...).
SIMUI = Target(
    dll="SIMUI.DLL",
    checks=("ui",),
    ui={
        "place_windows": 0x100148D1,            # cSC3MainUIMgr::place_windows
        "get_menu_btn_info_main": 0x1004C3E9,   # cSC3WinMenuBtnMain
        "get_layout_info": 0x100270E5,          # cSC3WinDateCashTitle
        "wm_global": 0x100BFABC,                # cached cRZWinManager, read by 0x10085091
        "wm_is_window": 0x2C,                   # IsWindowValid(cIGZWin*)
        "mgr_root": 0x1C,                       # cSC3MainUIMgr: the main window
        "get_area": 0xAC,                       # cIGZWin slots
        "get_child": 0x78,
        "win_width": 0x90,
        "win_height": 0x94,
        "btn_parent": 0x20,                     # GetParentWin
        "btn_id": 0xE8,                         # GetID
        "dct_parent": 0x10,                     # get_layout_info's way up to the screen
        "dct_parent_up": 0x0C,
        "id_panel": 0x42FB7DEC,
        "id_nav": 0x42FB7DED,
        "id_rci": 0x42FB7DEA,
        "id_bar": 0x42FB7DEB,
    },
)


SIMNTWRK = Target(
    dll="SIMNTWRK.DLL",
    checks=("tiling",),
    tiling={
        # Static, stdcall: (cRZFile*, char* buffer, vector<cGZResourceKey>*), ret 0xC, bool in AL.
        "GrokFileTileSet": 0x1001746E,
        "buffer_size": 0x20000,         # zeroes [1, 0x1FFFF] before and after
        # cRZFile slots, called by the read helper at 0x10017596 (the Loki slots minus 8).
        "file_check": 0x54,             # no arguments, bool; false fails the read
        "file_open": 0x0C,              # (1, 2, 1)
        "file_read": 0x3C,              # (buffer, &length); length goes in as 0x1FFFF
        "file_close": 0x14,
        # vector: begin, end, capacity; 12-byte cGZResourceKey (type, group, instance).
        "vec_begin": 0x0,
        "vec_end": 0x4,
        "vec_cap": 0x8,
        "key_size": 0xC,
        "key_type": 0xE223741F,         # the occupant key every entry gets
        "key_group": 0xA317745F,
    },
)

TARGETS = [SIMDIRT, SIMBABLD, SIMUI, SIMNTWRK]
