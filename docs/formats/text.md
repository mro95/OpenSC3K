# String tables

`Apps/Res/Text/<LANG>/*.IXF`. Each IXF entry is one string of type `2026960B`, addressed by
(group = table, instance = string id).
Id `FFFFFFFF` holds a placeholder such as `NEED SC3_STRINGTABLE_MENU STRING HERE`.

```
u32 length
u8  text[length]     Windows-1252, no terminator
```

Parser: `crates/sc3k-formats/src/text.rs`; lookup: `Assets::string(table, id)`.

| Table | File | Examples |
|---|---|---|
| `03C09AFF` | `SC3StringsMenu.IXF` | 1 Start New City, 2 Load City, 4 Preferences, 6 Exit, 9 Starter Town, 0x13 Real City Terrain |
| `041F2625` | `SEStringsUI.IXF` | 1 Play Scenario, 2 Load Scenario, 0x197 the 2000 copyright line |
| `63DE4715` | `BATStringsMain.IXF` | Building Architect |
