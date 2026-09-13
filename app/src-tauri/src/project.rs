//! Project model (.rigpilot, JSON — machine-written, never hand-edited).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MIDI ticks per quarter note, fixed for projects and exports.
pub const PPQN: u32 = 960;

/// Current project file format. Older files are refused, never converted.
pub const FORMAT_VERSION: u32 = 2;

fn default_volume() -> f32 {
    1.0
}

fn default_resolution_ms() -> u32 {
    10
}

fn default_true() -> bool {
    true
}

fn default_lane_height() -> u32 {
    96
}

fn legacy_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(default = "legacy_version")]
    pub format_version: u32,
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
    /// One-Shot and Hold events on the anonymous lanes.
    #[serde(default)]
    pub events: Vec<Event>,
    /// At most one curve per Automation Command of the track's Device.
    #[serde(default)]
    pub automation: Vec<AutomationCurve>,
    #[serde(default)]
    pub automation_view: AutomationView,
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
}

/// The value curve of one Automation Command. Created on first edit and kept
/// afterwards; a curve with no breakpoints means "not automated" and exports
/// nothing, so the knob keeps whatever the player set on the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationCurve {
    pub command_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Minimum spacing between exported CC steps, in milliseconds (converted to
    /// ticks per project BPM at export). Lower = smoother, higher = fewer
    /// messages on a busy MIDI bus.
    #[serde(default = "default_resolution_ms")]
    pub resolution_ms: u32,
    /// Re-send the current value every N ms even when it hasn't changed, so a
    /// single dropped CC can't leave the device on the wrong value. 0 = off.
    #[serde(default)]
    pub resend_ms: u32,
    /// Sorted by tick, strictly increasing.
    #[serde(default)]
    pub breakpoints: Vec<Breakpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakpoint {
    /// Absolute song tick.
    pub tick: u64,
    pub value: u8,
    /// Shape of the segment *arriving at* this point, FL-Studio style (ignored on
    /// the first one, which has nothing before it).
    #[serde(default)]
    pub shape: Shape,
    /// `Curve` only: -1.0..=1.0, 0 = straight.
    #[serde(default)]
    pub tension: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    #[default]
    Linear,
    Curve,
    Hold,
}

/// Which curve a track's Automation Lane shows, and how tall it is. View state:
/// saved with the project, restored across undo (see the frontend history).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationView {
    /// Command shown in the lane; `None` = the lane is collapsed.
    #[serde(default)]
    pub command: Option<String>,
    /// Lane height in px.
    #[serde(default = "default_lane_height")]
    pub height: u32,
}

impl Default for AutomationView {
    fn default() -> Self {
        Self {
            command: None,
            height: default_lane_height(),
        }
    }
}

/// Value of a curve at an absolute tick, or `None` when it has no breakpoints.
///
/// Once a curve has any point it has a value everywhere: the first value is held
/// back to the song start and the last one is held to the end. `forced` overrides
/// every segment's stored shape — a definition can restrict a target to one curve
/// type, and a discrete value set always means `Hold`.
///
/// `bps` must be sorted by tick.
pub fn value_at(bps: &[Breakpoint], tick: u64, forced: Option<Shape>) -> Option<u8> {
    let first = bps.first()?;
    let last = bps.last().expect("non-empty");
    if tick <= first.tick {
        return Some(first.value);
    }
    if tick >= last.tick {
        return Some(last.value);
    }
    let i = bps.partition_point(|b| b.tick <= tick) - 1;
    let (a, b) = (&bps[i], &bps[i + 1]);
    if b.tick <= a.tick {
        return Some(b.value);
    }
    let u = (tick - a.tick) as f64 / (b.tick - a.tick) as f64;
    // The shape belongs to the point the segment arrives at.
    let shape = forced.unwrap_or(b.shape);
    Some(match shape {
        Shape::Hold => a.value,
        Shape::Linear => lerp(a.value, b.value, u),
        Shape::Curve => lerp(a.value, b.value, ease(u, b.tension as f64)),
    })
}

fn lerp(v0: u8, v1: u8, u: f64) -> u8 {
    (v0 as f64 + (v1 as f64 - v0 as f64) * u)
        .round()
        .clamp(0.0, 127.0) as u8
}

/// Power ease for `Shape::Curve`. Closed form in `u`, so a value can be read at
/// any tick without inverting a curve parameter (which a Bézier would need).
/// `k` is the stored tension, -1..=1; `ease(u, 0) == u`.
pub fn ease(u: f64, k: f64) -> f64 {
    let k = k.clamp(-1.0, 1.0);
    let p = 1.0 + 3.0 * k.abs();
    if k >= 0.0 {
        u.powf(p)
    } else {
        1.0 - (1.0 - u).powf(p)
    }
}

pub fn save(path: &str, project: &Project) -> Result<(), String> {
    // Always stamp the current version, whatever the frontend sent.
    let mut project = project.clone();
    project.format_version = FORMAT_VERSION;
    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("could not write {path}: {e}"))
}

pub fn load(path: &str) -> Result<Project, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("invalid project file: {e}"))?;
    // Check the version before serde sees the body, so an old project gets a
    // sentence instead of a parser error. There is no conversion by design.
    let version = value
        .get("formatVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(legacy_version() as u64) as u32;
    if version < FORMAT_VERSION {
        return Err(
            "This project was made with an older version of RigPilot and can't be opened.".into(),
        );
    }
    if version > FORMAT_VERSION {
        return Err(
            "This project was made with a newer version of RigPilot. Update RigPilot to open it."
                .into(),
        );
    }
    serde_json::from_value(value).map_err(|e| format!("invalid project file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bp(tick: u64, value: u8, shape: Shape, tension: f32) -> Breakpoint {
        Breakpoint {
            tick,
            value,
            shape,
            tension,
        }
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("rigpilot-project-test-{name}.rigpilot"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn frontend_snapshot_preserves_tempo() {
        // Shape exactly as the Svelte frontend sends via $state.snapshot(project).
        let json = r#"{
            "formatVersion": 2,
            "name": "Song",
            "bpm": 90,
            "timeSignature": [3, 4],
            "tracks": []
        }"#;
        let p: Project = serde_json::from_str(json).unwrap();
        assert_eq!(p.bpm, 90.0);
        assert_eq!(p.time_signature, (3, 4));
    }

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

    #[test]
    fn automation_json_uses_camel_case_and_kebab_shapes() {
        let curve = AutomationCurve {
            command_id: "volume-pedal".into(),
            enabled: true,
            resolution_ms: 10,
            breakpoints: vec![bp(0, 0, Shape::Hold, 0.0)],
            resend_ms: 0,
        };
        let json = serde_json::to_string(&curve).unwrap();
        assert!(json.contains("\"commandId\""), "{json}");
        assert!(json.contains("\"resolutionMs\""), "{json}");
        assert!(json.contains("\"shape\":\"hold\""), "{json}");
    }

    #[test]
    fn empty_curve_has_no_value() {
        assert_eq!(value_at(&[], 0, None), None);
    }

    #[test]
    fn value_is_held_outside_the_drawn_range() {
        let bps = [bp(1000, 40, Shape::Linear, 0.0), bp(2000, 90, Shape::Linear, 0.0)];
        // held back to the song start
        assert_eq!(value_at(&bps, 0, None), Some(40));
        assert_eq!(value_at(&bps, 999, None), Some(40));
        // held to the end
        assert_eq!(value_at(&bps, 2000, None), Some(90));
        assert_eq!(value_at(&bps, 999_999, None), Some(90));
    }

    #[test]
    fn linear_segment_interpolates() {
        let bps = [bp(0, 0, Shape::Linear, 0.0), bp(1000, 100, Shape::Linear, 0.0)];
        assert_eq!(value_at(&bps, 250, None), Some(25));
        assert_eq!(value_at(&bps, 500, None), Some(50));
        assert_eq!(value_at(&bps, 750, None), Some(75));
    }

    #[test]
    fn hold_segment_keeps_the_left_value_until_the_next_point() {
        let bps = [bp(0, 10, Shape::Linear, 0.0), bp(1000, 100, Shape::Hold, 0.0)];
        assert_eq!(value_at(&bps, 1, None), Some(10));
        assert_eq!(value_at(&bps, 999, None), Some(10));
        assert_eq!(value_at(&bps, 1000, None), Some(100));
    }

    #[test]
    fn curve_with_zero_tension_equals_linear() {
        let curved = [bp(0, 0, Shape::Linear, 0.0), bp(1000, 100, Shape::Curve, 0.0)];
        let linear = [bp(0, 0, Shape::Linear, 0.0), bp(1000, 100, Shape::Linear, 0.0)];
        for t in [1u64, 250, 500, 750, 999] {
            assert_eq!(value_at(&curved, t, None), value_at(&linear, t, None), "at {t}");
        }
    }

    #[test]
    fn curve_tension_bows_both_ways() {
        let slow = [bp(0, 0, Shape::Linear, 0.0), bp(1000, 100, Shape::Curve, 1.0)];
        let fast = [bp(0, 0, Shape::Linear, 0.0), bp(1000, 100, Shape::Curve, -1.0)];
        // p = 4: 0.5^4 = 0.0625 -> 6 ; 1 - 0.5^4 = 0.9375 -> 94
        assert_eq!(value_at(&slow, 500, None), Some(6));
        assert_eq!(value_at(&fast, 500, None), Some(94));
        // endpoints are untouched by tension
        assert_eq!(value_at(&slow, 0, None), Some(0));
        assert_eq!(value_at(&slow, 1000, None), Some(100));
    }

    #[test]
    fn a_forced_shape_overrides_what_the_points_store() {
        let bps = [bp(0, 0, Shape::Linear, 0.0), bp(1000, 127, Shape::Linear, 0.0)];
        assert_eq!(value_at(&bps, 500, Some(Shape::Hold)), Some(0));
        assert_eq!(value_at(&bps, 1000, Some(Shape::Hold)), Some(127));
    }

    #[test]
    fn ease_is_identity_at_zero_tension() {
        for u in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((ease(u, 0.0) - u).abs() < 1e-12, "u={u}");
        }
    }

    #[test]
    fn save_stamps_the_current_format_version() {
        let path = tmp("stamp");
        let project = Project {
            format_version: 1, // whatever the caller had
            name: "S".into(),
            bpm: 120.0,
            time_signature: (4, 4),
            tracks: vec![],
        };
        save(path.to_str().unwrap(), &project).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("\"formatVersion\": 2"), "{written}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_refuses_older_and_versionless_projects() {
        for body in [
            r#"{"name":"Old","bpm":120,"timeSignature":[4,4],"tracks":[]}"#,
            r#"{"formatVersion":1,"name":"Old","bpm":120,"timeSignature":[4,4],"tracks":[]}"#,
        ] {
            let path = tmp("old");
            std::fs::write(&path, body).unwrap();
            let err = load(path.to_str().unwrap()).unwrap_err();
            assert!(err.contains("older version of RigPilot"), "{err}");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn load_refuses_newer_projects() {
        let path = tmp("new");
        std::fs::write(
            &path,
            r#"{"formatVersion":99,"name":"N","bpm":120,"timeSignature":[4,4],"tracks":[]}"#,
        )
        .unwrap();
        let err = load(path.to_str().unwrap()).unwrap_err();
        assert!(err.contains("newer version of RigPilot"), "{err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_load_round_trip_keeps_curves_and_view() {
        let path = tmp("roundtrip");
        let project = Project {
            format_version: FORMAT_VERSION,
            name: "S".into(),
            bpm: 100.0,
            time_signature: (4, 4),
            tracks: vec![Track::Midi(MidiTrack {
                name: "K".into(),
                definition_id: "kemper-profiler-stage".into(),
                midi_channel: 2,
                latency_ms: 5,
                mute: false,
                solo: false,
                events: vec![],
                automation: vec![AutomationCurve {
                    command_id: "volume-pedal".into(),
                    enabled: false,
                    resolution_ms: 20,
                    resend_ms: 500,
                    breakpoints: vec![
                        bp(0, 10, Shape::Curve, -0.5),
                        bp(960, 120, Shape::Hold, 0.0),
                    ],
                }],
                automation_view: AutomationView {
                    command: Some("volume-pedal".into()),
                    height: 140,
                },
            })],
        };
        save(path.to_str().unwrap(), &project).unwrap();
        let back = load(path.to_str().unwrap()).unwrap();
        let Track::Midi(t) = &back.tracks[0] else {
            panic!("expected a MIDI track")
        };
        assert_eq!(t.automation[0].command_id, "volume-pedal");
        assert!(!t.automation[0].enabled);
        assert_eq!(t.automation[0].resolution_ms, 20);
        assert_eq!(t.automation[0].resend_ms, 500);
        assert_eq!(t.automation[0].breakpoints[0].shape, Shape::Curve);
        assert_eq!(t.automation[0].breakpoints[0].tension, -0.5);
        assert_eq!(t.automation_view.command.as_deref(), Some("volume-pedal"));
        assert_eq!(t.automation_view.height, 140);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn automation_defaults_fill_in_for_a_bare_track() {
        let json = r#"{
            "formatVersion": 2,
            "name": "S", "bpm": 120, "timeSignature": [4, 4],
            "tracks": [{
                "type": "midi", "name": "K", "definitionId": "d", "midiChannel": 1
            }]
        }"#;
        let p: Project = serde_json::from_str(json).unwrap();
        let Track::Midi(t) = &p.tracks[0] else {
            panic!("expected a MIDI track")
        };
        assert!(t.automation.is_empty());
        assert_eq!(t.automation_view.command, None);
        assert_eq!(t.automation_view.height, 96);
    }
}
