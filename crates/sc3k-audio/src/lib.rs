//! Sound output, modelled on the original's `cBoxX` (`AUDIO.DLL` / Loki `libAudio.so`).
//!
//! For now only the "all-purpose sound" slot exists: one sound at a time, identified by a
//! caller-chosen id (`cBoxX::PlayAllPurposeSnd`). The menu music uses it.

use rodio::source::Source;
use rodio::buffer::SamplesBuffer;
use rodio::{MixerDeviceSink, Player};
use sc3k_formats::wav::Wav;
use std::num::NonZero;

/// Volumes use the original's scale: 0..=1024 (`AUDIO.INI`, the music volume setting).
pub const MAX_VOLUME: u32 = 1024;

/// Id of the menu loop in the all-purpose slot (`cBoxX::UpdateMusic`).
pub const MENU_LOOP_ID: u32 = 2;
/// Path of the menu loop under `Apps/Res/Sound` (hard-coded in `AUDIO.DLL`).
pub const MENU_LOOP_PATH: &str = "Music/3kloop.wav";

pub struct Audio {
    device: MixerDeviceSink,
    all_purpose: Option<(u32, Player)>,
}

impl Audio {
    /// Open the default output device.
    pub fn open() -> Result<Audio, String> {
        let mut device = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        device.log_on_drop(false);
        Ok(Audio { device, all_purpose: None })
    }

    /// `cBoxX::PlayAllPurposeSnd`: if `id` is already playing, only its volume changes;
    /// otherwise the current all-purpose sound stops and `wav` starts. Volume 0 stops it.
    pub fn play_all_purpose(&mut self, id: u32, wav: &Wav, looped: bool, volume: u32) {
        if volume == 0 {
            self.stop_all_purpose();
            return;
        }
        let gain = volume.min(MAX_VOLUME) as f32 / MAX_VOLUME as f32;
        if let Some((current, player)) = &self.all_purpose {
            if *current == id {
                player.set_volume(gain);
                return;
            }
        }
        self.stop_all_purpose();
        let (Some(channels), Some(rate)) = (NonZero::new(wav.channels), NonZero::new(wav.sample_rate)) else {
            return;
        };
        let samples: Vec<f32> = wav.samples.iter().map(|&s| s as f32 / 32768.0).collect();
        let buffer = SamplesBuffer::new(channels, rate, samples);
        let player = Player::connect_new(self.device.mixer());
        player.set_volume(gain);
        if looped {
            player.append(buffer.repeat_infinite());
        } else {
            player.append(buffer);
        }
        self.all_purpose = Some((id, player));
    }

    /// `cBoxX::KillAllPurposeSnd`.
    pub fn stop_all_purpose(&mut self) {
        if let Some((_, player)) = self.all_purpose.take() {
            player.stop();
        }
    }

    /// Id of the all-purpose sound that is playing, if any.
    pub fn all_purpose_id(&self) -> Option<u32> {
        self.all_purpose.as_ref().filter(|(_, p)| !p.empty()).map(|(id, _)| *id)
    }
}
