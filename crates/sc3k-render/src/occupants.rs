//! Occupant sprites: the pictures of anything that stands on a cell (trees, network tiles).
//! See `docs/formats/occupant.md`; trees in `docs/render/flora.md`, roads in
//! `docs/render/roads.md`.
//!
//! An occupant key resolves in three steps, each through the caller's `find`:
//! 1. the occupant record (`OccupantAttribs.IXF`), whose property 0x67 names
//! 2. the sprite attributes (`CSATTRIB.IXF`), which list the sprites by frame
//! 3. in a sprite archive (`.DAT`, the entry's group).
//!
//! The frame is `layer · 20 + zoom · 4 + rotation` for every record seen so far; picking it
//! is left to the caller.

use sc3k_formats::{
    csattrib::{SpriteAttrib, TYPE_SPRITE_ATTRIB},
    ixf::Tgi,
    occupant::{Occupant, PROP_SPRITE_ATTRIB},
    sprite::{self, ImageInfo},
};
use sc3k_ui::Sprite;

pub(crate) struct Image {
    pub(crate) sprite: Sprite,
    pub(crate) info: ImageInfo,
}

pub(crate) fn load_occupant<'a>(
    find: &impl Fn(Tgi) -> Option<&'a [u8]>,
    key: Tgi,
) -> Result<Vec<Option<Image>>, String> {
    let occupant = Occupant::parse(find(key).ok_or_else(|| format!("occupant {key}"))?)
        .map_err(|e| format!("occupant {key}: {e}"))?;
    let attrib_key = occupant
        .key(PROP_SPRITE_ATTRIB)
        .ok_or_else(|| format!("occupant {key}: no sprite key"))?;
    if attrib_key.type_id != TYPE_SPRITE_ATTRIB {
        return Err(format!("occupant {key}: sprite key {attrib_key}"));
    }
    let attrib = SpriteAttrib::parse(
        find(attrib_key).ok_or_else(|| format!("sprite attributes {attrib_key}"))?,
    )
    .map_err(|e| format!("sprite attributes {attrib_key}: {e}"))?;
    let mut out = Vec::with_capacity(attrib.entries.len());
    for e in &attrib.entries {
        let tgi = |type_id| Tgi {
            type_id,
            group_id: e.group,
            instance_id: e.instance,
        };
        let (Some(data), Some(info)) = (find(tgi(sprite::TYPE_DATA)), find(tgi(sprite::TYPE_INFO)))
        else {
            out.push(None);
            continue;
        };
        let sprite = match sprite::Sprite::parse(data)
            .map_err(|err| format!("sprite {}: {err}", tgi(0)))?
        {
            sprite::Sprite::Span(s) => Sprite::from_span(&s),
            sprite::Sprite::Alpha(_) => return Err(format!("sprite {} is an alpha mask", tgi(0))),
        };
        let info = ImageInfo::parse(info).map_err(|err| format!("image info {}: {err}", tgi(1)))?;
        out.push(Some(scale(Image { sprite, info }, e.scale)));
    }
    Ok(out)
}

/// `SprAttDraw`'s scale byte: positive multiplies the image info and the picture, negative
/// divides them.
fn scale(img: Image, s: i8) -> Image {
    let (num, den) = match s {
        0 => return img,
        s if s > 0 => (s as i32, 1),
        s => (1, -(s as i32)),
    };
    let f = |v: i16| (v as i32 * num / den) as i16;
    let i = img.info;
    Image {
        sprite: img.sprite.scaled(num, den),
        info: ImageInfo {
            left: f(i.left),
            up: f(i.up),
            right: f(i.right),
            down: f(i.down),
        },
    }
}
