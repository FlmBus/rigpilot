//! Project model (.rigpilot, JSON — machine-written, never hand-edited).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MIDI ticks per quarter note, fixed for projects and exports.
pub const PPQN: u32 = 960;

fn default_volume() -> f32 {
    1.0
}

fn default_resolution_ms() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub bpm: f64,
    pub time_signature: (u8, u8),
    #[serde(default)]
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Track {
    Audio(AudioTrack),
    Midi(MidiTrack),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    pub name: String,
    /// Path relative to the project file (copied on import).
    pub file: String,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    /// Shift of the audio against the musical grid, in ticks.
    #[serde(default)]
    pub offset_ticks: i64,
    /// Visual waveform amplification (view preference, no effect on playback).
    #[serde(default = "default_volume")]
    pub waveform_gain: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiTrack {
    pub name: String,
    pub definition_id: String,
    /// 1..=16 — the universal Device Setting.
    pub midi_channel: u8,
    /// Latency compensation in ms: messages are sent this much earlier at
    /// export so the audible change lands on the placed position.
    #[serde(default)]
    pub latency_ms: u32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Event {
    OneShot {
        command_id: String,
        tick: u64,
        #[serde(default)]
        params: HashMap<String, u8>,
        #[serde(default)]
        lane: u32,
    },
    Hold {
        command_id: String,
        tick: u64,
        length: u64,
        #[serde(default)]
        params: HashMap<String, u8>,
        #[serde(default)]
        lane: u32,
    },
    Automation {
        command_id: String,
        tick: u64,
        length: u64,
        /// (tick offset relative to event start, value); kept sorted by offset.
        breakpoints: Vec<(u64, u8)>,
        /// Minimum spacing between exported CC steps, in milliseconds (converted
        /// to ticks per project BPM at export). Lower = smoother, higher = fewer
        /// messages on a busy MIDI bus.
        #[serde(default = "default_resolution_ms")]
        resolution_ms: u32,
        #[serde(default)]
        lane: u32,
    },
}

pub fn save(path: &str, project: &Project) -> Result<(), String> {
    let json = serde_json::to_string_pretty(project).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("could not write {path}: {e}"))
}

pub fn load(path: &str) -> Result<Project, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("invalid project file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_json_uses_camel_case_fields() {
        let ev = Event::OneShot {
            command_id: "tap-tempo".into(),
            tick: 0,
            params: HashMap::new(),
            lane: 0,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"one-shot\""), "{json}");
        assert!(json.contains("\"commandId\""), "{json}");
    }
}
