# SYS.PAK

`Apps/Sys/SYS.PAK` packs the game's configuration INI files, such as `SC3CityScheme.ini`.
Parser: `crates/sc3k-formats/src/pak.rs`; INI dialect: `crates/sc3k-formats/src/ini.rs`.
Read a file with `Assets::sys_ini(name)`. Names match case-insensitively.

## Archive
```
u32 count
entry[count]:
  u32 name_length
  u8  name[name_length]
  u32 offset          start of the file's data
```
A file's data runs to the next higher offset, or to the end of the archive for the last one.

## File
Each file is stored as a list of lines with no line terminators:
```
u32 line_count
line[line_count]:
  u32 length
  u8  text[length]    Windows-1252
```

## INI dialect
- `[Section]` headers and `key=value` lines. Blank lines and lines starting with `;` are skipped.
- Keys and values are trimmed.
- Entries keep their file order. `cSC3CitySchemeMgr` lists schemes in the order they appear.
- Numbers (`ConvertToUint32`): decimal, or hexadecimal with a `0x` prefix. Lists are
  comma-separated.

## SC3CityScheme.ini
Read by `cSC3CitySchemeMgr`. Three sections list the choices in the New City dialog. The key is
the scheme ID; key 0 is the default.

| Section | Values (group, instance pairs) |
|---|---|
| `LandScapes` | icon image, palette, preview sprite |
| `FloraSets` | icon image, colour sprite, alpha mask |
| `BuildingSets` | icon image, colour sprite, alpha mask |

Loading the icon image from the first pair directly gives some wrong icons. For example,
FloraSets key 0 gets a palm icon but a conifer preview. The game resolves icons through
`cSC3MenuBtnDef::GetMenuBtnDefID`, which is not traced yet.
