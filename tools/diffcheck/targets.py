"""Addresses of the original functions under test, from docs/sim/random.md and
docs/sim/terrain-gen.md. A check only uses what is listed here."""

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
    rng: dict
    # [start, end) of the cRZRandom code: a call from inside it is nested, not logged.
    rng_code: tuple
    # Thunks that jump to an RNG method (address -> op name).
    rng_thunks: dict = field(default_factory=dict)
    # cSC3DirtGenerator. The vtable is found at run time from these slots.
    generator: dict = field(default_factory=dict)
    # Named stages of GenerateRandom, for saying where a trace diverged (address -> name).
    stages: dict = field(default_factory=dict)


SIMDIRT = Target(
    dll="SIMDIRT.DLL",
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
)
