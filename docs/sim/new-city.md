# Starting a new city

Code: `crates/sc3k-sim/src/city.rs` (`NewCityInfo`, `City`) and `Settings::new_city_info` in
`crates/sc3k-ui/src/new_city.rs`. The dialog itself is described in `docs/ui/new-city.md`.

Addresses are from the Loki build (libSimUI, libSimInit, libSimCity, libSimMisc) unless
marked Windows.

## Order of events
1. **OK in the dialog.** `cSC3WinProcCityScheme::ReadValuesFromWindow` (libSimUI 0x18ADE4) reads
   the controls. In new-city mode (field `+0xE4` = 1) it also drives the dirt generator:
   1. `SetDifficulty(difficulty)`.
   2. If `IsReady()` is false: `Init(size, size)`, then
      `GenerateRandom(0xFFFFFFFF, 0x40, 0x40, 0x40, 0x24)`.

   **The terrain is generated here, before the city exists.** The seed `0xFFFFFFFF` means "seed
   from the clock" (`timeGetTime`, `docs/sim/random.md`), so every new city differs.
   - Windows SIMUI.DLL makes the same calls with the same arguments at 0x1005ED3A–0x1005ED72.
     There the generator is at dialog `+0x178` and `GenerateRandom` is vtable slot `0x1C`.
2. **`cSC3CmdSelectCityScheme`** (libSimUI) wraps the dialog. Its getters read the dialog's
   fields, with defaults when there is no dialog:

   | Getter | Dialog field | Default |
   |---|---|---|
   | `GetFunds` | `+0x13C` | 50,000 |
   | `GetDifficulty` | `+0x148` | 1 |
   | `GetCitySize` | `+0x150` | `0x100` |
   | `GetDirtGenerator` | `+0x154` | none |

3. **`cSC3CmdNewCity::Execute`** (libSimInit 0x4A834):
   1. If a city is open, it asks whether to save or quit it first.
   2. It creates a `cSC3NewCityInfo` and copies the dialog's values into it, in this order:
      name, mayor, funds, funds-are-debt, difficulty, size, start year, auto budget,
      disasters, dirt generator.
   3. It calls `DoGameNew(info)` (0x4B254), which hands the info to the app to build the city.
   4. It sends message `0x624A8220` and plays UI sound `0x419`.
4. **`cSC3City::Init(cISC3NewCityInfo&)`** (libSimCity 0x292C4):
   1. It shows a progress bar, captioned with string `029541F4/0x2DC`.
   2. It reads the X, Y and Z sizes, the city type and the dirt generator.
   3. It builds the date `cRZDate(1, 1, start year)`.
   4. It calls `Init(x, y, z, dirt generator)` (0x259D0):
      - It registers 13 message IDs and creates the clock (`cRZClock`).
      - It stores the sizes.
      - It keeps the generator at `+0x144` while `BaseInitLayers` builds the layers. The
        terrain layer (`cSC3DirtBag`) takes its altitudes from the generator; see
        `docs/sim/terrain-gen.md`.
      - It then releases the generator.
   5. It sets the date (vtable `0x1F8`, `0x200`), the name (`0x84`), the mayor (`0x94`), the
      difficulty (`0xBC`), disasters (`0xA4`) and auto budget (`0x9C`).
   6. It sets the funds through the budget layer (`+0xAC`):
      - With funds as debt: `SetTotalFunds(0)`, then `IssueBond(funds)`.
      - Otherwise: `SetTotalFunds(funds)`.

`cSC3City::DoNewCity(bool)` is not part of this path, despite its name. It checks the
`E3270FE9` setting and posts a warning message when the funds drop below −100,000.

## cSC3NewCityInfo
76 bytes. The vtable is at `__vt_15cSC3NewCityInfo`. The defaults are from the constructor
(0x4B364).

| Offset | Field | Default |
|---|---|---|
| `0x0C` | city name (`cRZString`) | "New City" (`029541F4/0x282`) |
| `0x18` | mayor name | "Defacto" (`029541F4/0x281`) |
| `0x20` | funds, `i64` | 0 |
| `0x28` | funds are debt | false |
| `0x2C`, `0x30`, `0x34` | X, Y, Z size | `0x80` each |
| `0x38` | start year | 1900 |
| `0x3C` | city type | 7 |
| `0x40` | auto budget | |
| `0x41` | disasters | |
| `0x44` | difficulty | 0 |
| `0x48` | dirt generator | none |

- `SetCitySize(n)` sets all three sizes to `n`, so Z equals the map size.
- The new-city path never sets the city type, so it stays 7. What it means is not known.

## Bonds (cSC3BudgetLayer, libSimMisc)
`IssueBond(amount)` (0x6C098):
1. The amount is rounded down to a multiple of 5,000.
2. The bond is refused if:
   - 10 bonds (`kMaxBonds`) are already out;
   - the rounded amount is 0 or above `kMaxBondAmt` (25,000);
   - the total borrowed would pass `GetCurrentBorrowingLimit`. The remake does not check this
     limit yet.
3. Otherwise it records a 28-byte bond:

   | Offset | Field |
   |---|---|
   | 0 | month of issue |
   | 4 | issue year + `kBondLifespan` (10) |
   | 8 | amount |
   | `0x0C` | total to repay: 10 × 12 × monthly payment |
   | `0x10` | remaining, starts at the total |
   | `0x14` | 0 |
   | `0x18` | monthly payment: amount × `kMonthlyPaymentPer1K` (15) / 1000 |

4. It adds the amount to the total borrowed and deposits it (vtable `0x20`).

A Hard city therefore starts with §10,000 in the treasury and one bond: §150 a month, §18,000
in all, paid off in the start year + 10.
