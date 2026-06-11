//! SMF export: one Type-0 file per MIDI track, PPQN 960, tempo + time signature
//! included, optional Reset Block at t=0 (see docs/plan.md §3).

use crate::definition::{self, Command, DeviceDefinition, Message};
use crate::project::{Event, MidiTrack, Project, Track, PPQN};
use midly::num::{u15, u24, u28, u4, u7};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub reset_block: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { reset_block: true }
    }
}

/// Minimum tick distance between rendered automation CC steps
/// (PPQN/96 = a 384th note; at 120 BPM that is ~5 ms).
const AUTOMATION_MIN_STEP: u64 = (PPQN / 96) as u64;

/// A resolved MIDI message at an absolute tick. `order` keeps emission stable
/// and deterministic for simultaneous events (reset block first, then lane order).
struct Emitted {
    tick: u64,
    order: u64,
    message: MidiMessage,
}

fn resolve_message(msg: &Message, params: &HashMap<String, u8>) -> Result<MidiMessage, String> {
    Ok(match msg {
        Message::NoteOn(m) => MidiMessage::NoteOn {
            key: u7::new(definition::resolve_value(&m.note, params)?),
            vel: u7::new(definition::resolve_value(&m.velocity, params)?),
        },
        Message::NoteOff(m) => MidiMessage::NoteOff {
            key: u7::new(definition::resolve_value(&m.note, params)?),
            vel: u7::new(definition::resolve_value(&m.velocity, params)?),
        },
        Message::ControlChange(m) => MidiMessage::Controller {
            controller: u7::new(m.controller),
            value: u7::new(definition::resolve_value(&m.value, params)?),
        },
        Message::ProgramChange(m) => MidiMessage::ProgramChange {
            program: u7::new(definition::resolve_value(&m.program, params)?),
        },
    })
}

fn ms_to_ticks(ms: u32, bpm: f64) -> u64 {
    ((ms as f64 / 1000.0) * (bpm / 60.0) * PPQN as f64).round() as u64
}

fn command_latency(cmd: &Command) -> u32 {
    match cmd {
        Command::OneShot(c) => c.latency.unwrap_or(0),
        Command::Hold(c) => c.latency.unwrap_or(0),
        Command::Automation(c) => c.latency.unwrap_or(0),
    }
}

fn find_command<'a>(def: &'a DeviceDefinition, id: &str) -> Result<&'a Command, String> {
    def.commands
        .items
        .iter()
        .find(|c| c.id() == id)
        .ok_or_else(|| format!("definition '{}' has no command '{}'", def.id, id))
}

/// Renders one automation event into CC steps (linear segments between breakpoints).
fn render_automation(
    controller: u8,
    start: u64,
    breakpoints: &[(u64, u8)],
    order: &mut u64,
    out: &mut Vec<Emitted>,
) {
    let mut emit = |tick: u64, value: u8, order: &mut u64| {
        out.push(Emitted {
            tick,
            order: *order,
            message: MidiMessage::Controller {
                controller: u7::new(controller),
                value: u7::new(value),
            },
        });
        *order += 1;
    };

    let mut last_value: Option<u8> = None;
    for pair in breakpoints.windows(2) {
        let (t0, v0) = pair[0];
        let (t1, v1) = pair[1];
        if last_value.is_none() {
            emit(start + t0, v0, order);
            last_value = Some(v0);
        }
        if t1 <= t0 {
            continue;
        }
        // Step through the segment, emitting on value change with a minimum spacing.
        let mut t = t0;
        while t < t1 {
            t = (t + AUTOMATION_MIN_STEP).min(t1);
            let value = (v0 as f64
                + (v1 as f64 - v0 as f64) * ((t - t0) as f64 / (t1 - t0) as f64))
                .round() as u8;
            if last_value != Some(value) {
                emit(start + t, value, order);
                last_value = Some(value);
            }
        }
    }
    if breakpoints.len() == 1 {
        emit(start + breakpoints[0].0, breakpoints[0].1, order);
    }
}

/// Resolves a MIDI track into absolutely-timed MIDI messages (without channel).
fn resolve_track(
    track: &MidiTrack,
    def: &DeviceDefinition,
    bpm: f64,
    options: &ExportOptions,
) -> Result<Vec<Emitted>, String> {
    let mut out: Vec<Emitted> = Vec::new();
    let mut order: u64 = 0;
    let track_lat = track.latency_ms;
    // Latency compensation: send messages early so the audible change lands
    // where the event was placed.
    let shift = |tick: u64, cmd: &Command| -> u64 {
        tick.saturating_sub(ms_to_ticks(track_lat + command_latency(cmd), bpm))
    };

    // ---- Reset Block at t=0: Init messages + Disengage of every Hold used ----
    if options.reset_block {
        if let Some(init) = &def.init {
            for msg in &init.messages {
                out.push(Emitted {
                    tick: 0,
                    order,
                    message: resolve_message(msg, &HashMap::new())?,
                });
                order += 1;
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        for event in &track.events {
            if let Event::Hold { command_id, params, .. } = event {
                if seen.contains(&command_id.as_str()) {
                    continue;
                }
                seen.push(command_id);
                if let Command::Hold(cmd) = find_command(def, command_id)? {
                    for msg in &cmd.disengage.messages {
                        out.push(Emitted {
                            tick: 0,
                            order,
                            message: resolve_message(msg, params)?,
                        });
                        order += 1;
                    }
                }
            }
        }
    }

    // ---- Events ----
    for event in &track.events {
        match event {
            Event::OneShot { command_id, tick, params, .. } => {
                let command = find_command(def, command_id)?;
                let Command::OneShot(cmd) = command else {
                    return Err(format!("'{command_id}' is not a One-Shot command"));
                };
                let at = shift(*tick, command);
                for msg in cmd.messages() {
                    out.push(Emitted {
                        tick: at,
                        order,
                        message: resolve_message(&msg, params)?,
                    });
                    order += 1;
                }
            }
            Event::Hold { command_id, tick, length, params, .. } => {
                let command = find_command(def, command_id)?;
                let Command::Hold(cmd) = command else {
                    return Err(format!("'{command_id}' is not a Hold command"));
                };
                let on = shift(*tick, command);
                let off = shift(tick + length, command);
                for msg in &cmd.engage.messages {
                    out.push(Emitted {
                        tick: on,
                        order,
                        message: resolve_message(msg, params)?,
                    });
                    order += 1;
                }
                for msg in &cmd.disengage.messages {
                    out.push(Emitted {
                        tick: off,
                        order,
                        message: resolve_message(msg, params)?,
                    });
                    order += 1;
                }
            }
            Event::Automation { command_id, tick, breakpoints, .. } => {
                let command = find_command(def, command_id)?;
                let Command::Automation(cmd) = command else {
                    return Err(format!("'{command_id}' is not an Automation command"));
                };
                let mut bps = breakpoints.clone();
                bps.sort_by_key(|(t, _)| *t);
                render_automation(cmd.target.controller, shift(*tick, command), &bps, &mut order, &mut out);
            }
        }
    }

    out.sort_by_key(|e| (e.tick, e.order));
    Ok(out)
}

/// Builds the SMF (Type 0) bytes for one MIDI track.
pub fn track_to_smf(project: &Project, track: &MidiTrack, options: &ExportOptions) -> Result<Vec<u8>, String> {
    if !(1..=16).contains(&track.midi_channel) {
        return Err(format!("track '{}': MIDI channel must be 1-16", track.name));
    }
    let def = definition::find(&track.definition_id)?;
    let events = resolve_track(track, &def, project.bpm, options)?;
    let channel = u4::new(track.midi_channel - 1);

    let mut smf = Smf::new(Header::new(
        Format::SingleTrack,
        Timing::Metrical(u15::new(PPQN as u16)),
    ));

    let mut midi_track: Vec<TrackEvent> = Vec::new();
    let micros_per_beat = (60_000_000f64 / project.bpm).round() as u32;
    midi_track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(micros_per_beat))),
    });
    let (num, den) = project.time_signature;
    midi_track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::TimeSignature(num, den.ilog2() as u8, 24, 8)),
    });
    midi_track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::TrackName(track.name.as_bytes())),
    });

    let mut last_tick: u64 = 0;
    for e in &events {
        midi_track.push(TrackEvent {
            delta: u28::new((e.tick - last_tick) as u32),
            kind: TrackEventKind::Midi {
                channel,
                message: e.message,
            },
        });
        last_tick = e.tick;
    }
    midi_track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    smf.tracks.push(midi_track);

    let mut bytes = Vec::new();
    smf.write_std(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Exports every MIDI track of the project to `<out_dir>/<project> - <track>.mid`.
/// Returns the written file paths.
pub fn export(project: &Project, out_dir: &str, options: &ExportOptions) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    for track in &project.tracks {
        if let Track::Midi(t) = track {
            let bytes = track_to_smf(project, t, options)?;
            let file = format!("{}/{} - {}.mid", out_dir, sanitize(&project.name), sanitize(&t.name));
            std::fs::write(&file, bytes).map_err(|e| format!("could not write {file}: {e}"))?;
            written.push(file);
        }
    }
    Ok(written)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{MidiTrack, Project};

    fn test_project() -> (Project, MidiTrack) {
        // Bar 1 beat 1 = tick 0; at PPQN 960 one 4/4 bar = 3840 ticks.
        let track = MidiTrack {
            name: "Kemper".into(),
            definition_id: "kemper-profiler-stage".into(),
            midi_channel: 2,
            latency_ms: 0,
            mute: false,
            solo: false,
            events: vec![
                Event::OneShot {
                    command_id: "preselect-performance".into(),
                    tick: 0,
                    params: HashMap::from([("performance".into(), 5u8)]), // Performance 6
                    lane: 0,
                },
                Event::OneShot {
                    command_id: "load-slot-2".into(),
                    tick: 10,
                    params: HashMap::new(),
                    lane: 0,
                },
                Event::Hold {
                    command_id: "stomp-a".into(),
                    tick: 3840, // bar 2
                    length: 3840,
                    params: HashMap::new(),
                    lane: 1,
                },
                Event::Automation {
                    command_id: "volume-pedal".into(),
                    tick: 7680, // bar 3
                    length: 1920,
                    breakpoints: vec![(0, 0), (1920, 127)],
                    lane: 2,
                },
            ],
        };
        let project = Project {
            name: "Test Song".into(),
            bpm: 120.0,
            time_signature: (4, 4),
            tracks: vec![Track::Midi(track.clone())],
        };
        (project, track)
    }

    fn abs_events(smf: &Smf) -> Vec<(u64, u8, MidiMessage)> {
        let mut out = Vec::new();
        let mut tick = 0u64;
        for e in &smf.tracks[0] {
            tick += e.delta.as_int() as u64;
            if let TrackEventKind::Midi { channel, message } = e.kind {
                out.push((tick, channel.as_int(), message));
            }
        }
        out
    }

    #[test]
    fn exports_valid_smf_with_correct_timing() {
        let (project, track) = test_project();
        let bytes = track_to_smf(&project, &track, &ExportOptions::default()).unwrap();
        let smf = Smf::parse(&bytes).unwrap();

        assert_eq!(smf.header.format, Format::SingleTrack);
        assert_eq!(smf.header.timing, Timing::Metrical(u15::new(960)));

        // Tempo meta = 500000 µs/beat at 120 BPM
        assert!(smf.tracks[0].iter().any(|e| matches!(
            e.kind,
            TrackEventKind::Meta(MetaMessage::Tempo(t)) if t.as_int() == 500_000
        )));

        let events = abs_events(&smf);
        // All on channel 2 (stored 0-based as 1)
        assert!(events.iter().all(|(_, ch, _)| *ch == 1));

        // Reset block at tick 0: disengage of stomp-a (NRPN, 4 CCs) is present
        // before anything else; CC38=0 closes it.
        let at_zero: Vec<_> = events.iter().filter(|(t, _, _)| *t == 0).collect();
        assert!(at_zero.len() >= 5); // 4 reset CCs + preselect CC47
        assert!(matches!(
            at_zero[0].2,
            MidiMessage::Controller { controller, .. } if controller.as_int() == 99
        ));

        // Preselect performance: CC47 value 5 at tick 0 (after the reset block)
        assert!(at_zero.iter().any(|(_, _, m)| matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 47 && value.as_int() == 5
        )));

        // Load slot 2: CC51=1 at tick 10
        assert!(events.iter().any(|(t, _, m)| *t == 10 && matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 51 && value.as_int() == 1
        )));

        // Hold: engage NRPN ends with CC38=1 at bar 2 (tick 3840),
        // disengage CC38=0 at bar 3 (tick 7680)
        assert!(events.iter().any(|(t, _, m)| *t == 3840 && matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 38 && value.as_int() == 1
        )));
        assert!(events.iter().any(|(t, _, m)| *t == 7680 && matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 38 && value.as_int() == 0
        )));

        // Automation: CC7 ramp 0->127 between ticks 7680 and 9600,
        // monotonically increasing, ending at 127
        let ramp: Vec<_> = events
            .iter()
            .filter_map(|(t, _, m)| match m {
                MidiMessage::Controller { controller, value } if controller.as_int() == 7 => {
                    Some((*t, value.as_int()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(ramp.first().unwrap(), &(7680, 0));
        assert_eq!(ramp.last().unwrap(), &(9600, 127));
        assert!(ramp.windows(2).all(|w| w[0].1 < w[1].1 && w[0].0 < w[1].0));
        // dense enough to be smooth, sparse enough not to flood
        assert!(ramp.len() > 60 && ramp.len() <= 128);
    }

    #[test]
    fn latency_compensation_shifts_messages_early() {
        let (mut project, mut track) = test_project();
        // 100 ms at 120 BPM = 0.1 * 2 beats/s = 0.2 beats = 192 ticks
        track.latency_ms = 100;
        project.tracks = vec![Track::Midi(track.clone())];
        let bytes = track_to_smf(&project, &track, &ExportOptions { reset_block: false }).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        let events = abs_events(&smf);
        // load-slot-2 (CC51) was placed at tick 10 -> shifted to 0 (clamped)
        assert!(events.iter().any(|(t, _, m)| *t == 0 && matches!(
            m, MidiMessage::Controller { controller, .. } if controller.as_int() == 51
        )));
        // stomp-a engage end (CC38=1) was at 3840 -> now 3840 - 192 = 3648
        assert!(events.iter().any(|(t, _, m)| *t == 3648 && matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 38 && value.as_int() == 1
        )));
    }

    #[test]
    fn reset_block_can_be_disabled() {
        let (project, track) = test_project();
        let bytes =
            track_to_smf(&project, &track, &ExportOptions { reset_block: false }).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        let events = abs_events(&smf);
        let at_zero: Vec<_> = events.iter().filter(|(t, _, _)| *t == 0).collect();
        // only the preselect CC47 remains at tick 0
        assert_eq!(at_zero.len(), 1);
    }

    #[test]
    fn example_project_loads_and_exports() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/cable-test.rigpilot"
        );
        let project = crate::project::load(path).unwrap();
        let out = std::env::temp_dir().join("rigpilot-export-test");
        std::fs::create_dir_all(&out).unwrap();
        let files = export(&project, out.to_str().unwrap(), &ExportOptions::default()).unwrap();
        assert_eq!(files.len(), 1);
        let bytes = std::fs::read(&files[0]).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        assert!(smf.tracks[0].len() > 20);
    }

    #[test]
    fn all_bundled_definitions_parse() {
        let defs = definition::bundled().unwrap();
        assert_eq!(defs.len(), 5);
        let kemper = defs.iter().find(|d| d.id == "kemper-profiler-stage").unwrap();
        assert!(kemper.commands.items.len() > 30);
        // the performance param carries its 125 labels
        let Command::OneShot(pre) = find_command(kemper, "preselect-performance").unwrap() else {
            panic!()
        };
        let param = pre.params().next().unwrap();
        assert_eq!(param.labels.len(), 125);
        assert_eq!(param.labels[5].text, "Performance 6");
    }
}
