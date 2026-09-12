//! SMF export: one Type-0 file per MIDI track, PPQN 960, tempo + time signature
//! included, optional Reset Block at t=0 (see docs/plan.md §3).

use crate::definition::{self, Command, DeviceDefinition, Message, Target};
use crate::project::{self, Breakpoint, Event, MidiTrack, Project, Shape, Track, PPQN};
use midly::num::{u15, u24, u28, u4, u7};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub reset_block: bool,
    /// How far the song runs, for curves that re-send their value periodically.
    /// The frontend knows the reference audio's length; 0 means "derive it from
    /// the project's own content".
    #[serde(default)]
    pub song_end_ticks: u64,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            reset_block: true,
            song_end_ticks: 0,
        }
    }
}

/// Last tick anything happens on, used when the caller gives no song length.
fn content_end(project: &Project) -> u64 {
    let mut end = 0;
    for track in &project.tracks {
        let Track::Midi(t) = track else { continue };
        for event in &t.events {
            end = end.max(match event {
                Event::OneShot { tick, .. } => *tick,
                Event::Hold { tick, length, .. } => tick + length,
            });
        }
        for curve in &t.automation {
            if let Some(last) = curve.breakpoints.last() {
                end = end.max(last.tick);
            }
        }
    }
    end
}

fn song_end(project: &Project, options: &ExportOptions) -> u64 {
    if options.song_end_ticks > 0 {
        options.song_end_ticks
    } else {
        content_end(project)
    }
}

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

/// The MIDI a target sends for one value. A target with declared messages uses
/// them, with `$value` standing in for the value; otherwise it is a plain CC.
fn target_messages(target: &Target, value: u8) -> Result<Vec<MidiMessage>, String> {
    let declared: Vec<Message> = target.declared_messages().collect();
    if declared.is_empty() {
        let controller = target
            .controller
            .ok_or("an automation target needs either a controller or messages")?;
        return Ok(vec![MidiMessage::Controller {
            controller: u7::new(controller),
            value: u7::new(value),
        }]);
    }
    let params = HashMap::from([("value".to_string(), value)]);
    declared.iter().map(|m| resolve_message(m, &params)).collect()
}

/// Keeps a value inside the target's declared range (a bad definition with
/// min > max is left alone rather than panicking in `clamp`).
fn clamp_to_target(target: &Target, value: u8) -> u8 {
    if target.min <= target.max {
        value.clamp(target.min, target.max)
    } else {
        value
    }
}

/// Renders one Automation Lane's curve into CC steps.
///
/// A curve with breakpoints has a value everywhere, so the first value is written
/// at the song start and the last one is simply left standing (a CC holds on the
/// device by itself). `Hold` segments and discrete targets emit one message at the
/// next breakpoint; `Linear`/`Curve` segments are sampled at `resolution_ms` and
/// emitted only where the rounded value actually changes.
///
/// A segment's shape is stored on the breakpoint it arrives at (FL-Studio style).
///
/// `shift` is the caller's latency compensation, applied per emitted tick.
#[allow(clippy::too_many_arguments)]
fn render_curve(
    target: &Target,
    breakpoints: &[Breakpoint],
    resolution_ms: u32,
    resend_ms: u32,
    bpm: f64,
    song_end: u64,
    shift: &dyn Fn(u64) -> u64,
    order: &mut u64,
    out: &mut Vec<Emitted>,
) -> Result<(), String> {
    let Some(first) = breakpoints.first() else {
        return Ok(());
    };
    let stepped = !target.step_values().is_empty();
    let forced = target.forced_shape();
    let min_step = ms_to_ticks(resolution_ms, bpm).max(1);

    // Collected unshifted, so the re-send pass can reason about real song time.
    let mut msgs: Vec<(u64, u8)> = Vec::new();
    let mut last: Option<u8> = None;
    let push = |tick: u64, raw: u8, msgs: &mut Vec<(u64, u8)>, last: &mut Option<u8>| {
        let value = if stepped {
            target.snap(raw)
        } else {
            clamp_to_target(target, raw)
        };
        if *last == Some(value) {
            return;
        }
        msgs.push((tick, value));
        *last = Some(value);
    };

    // The first value is in effect from the song start, not from the first point.
    push(0, first.value, &mut msgs, &mut last);

    for pair in breakpoints.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if b.tick <= a.tick {
            continue;
        }
        let shape = forced.unwrap_or(b.shape);
        if shape == Shape::Hold {
            push(b.tick, b.value, &mut msgs, &mut last);
            continue;
        }
        let mut t = a.tick;
        while t < b.tick {
            t = (t + min_step).min(b.tick);
            let value = project::value_at(breakpoints, t, forced).unwrap_or(b.value);
            push(t, value, &mut msgs, &mut last);
        }
    }

    if resend_ms > 0 {
        msgs = with_resends(&msgs, ms_to_ticks(resend_ms, bpm).max(1), song_end);
    }

    for (tick, value) in msgs {
        for message in target_messages(target, value)? {
            out.push(Emitted {
                tick: shift(tick),
                order: *order,
                message,
            });
            *order += 1;
        }
    }
    Ok(())
}

/// Repeats the standing value whenever `every` ticks pass without a message, so a
/// dropped CC cannot leave the device stuck on the wrong value. The value itself
/// never changes — these are duplicates, not extra curve detail.
fn with_resends(msgs: &[(u64, u8)], every: u64, song_end: u64) -> Vec<(u64, u8)> {
    let mut out: Vec<(u64, u8)> = Vec::with_capacity(msgs.len());
    for (i, &(tick, value)) in msgs.iter().enumerate() {
        out.push((tick, value));
        let next = msgs.get(i + 1).map(|m| m.0).unwrap_or(song_end);
        let mut t = tick.saturating_add(every);
        while t < next {
            out.push((t, value));
            t += every;
        }
    }
    out
}

/// Resolves a MIDI track into absolutely-timed MIDI messages (without channel).
fn resolve_track(
    track: &MidiTrack,
    def: &DeviceDefinition,
    bpm: f64,
    song_end: u64,
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
        }
    }

    // ---- Automation Lanes ----
    // Rendered after the events, so at equal ticks the emission order is
    // Reset Block -> Events -> automation, deterministically.
    for curve in &track.automation {
        if !curve.enabled || curve.breakpoints.is_empty() {
            continue;
        }
        let command = find_command(def, &curve.command_id)?;
        let Command::Automation(cmd) = command else {
            return Err(format!(
                "'{}' is not an Automation command",
                curve.command_id
            ));
        };
        let mut bps = curve.breakpoints.clone();
        bps.sort_by_key(|b| b.tick);
        bps.dedup_by_key(|b| b.tick);
        let shift_curve = |tick: u64| shift(tick, command);
        render_curve(
            &cmd.target,
            &bps,
            curve.resolution_ms,
            curve.resend_ms,
            bpm,
            song_end,
            &shift_curve,
            &mut order,
            &mut out,
        )?;
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
    let events = resolve_track(track, &def, project.bpm, song_end(project, options), options)?;
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

/// One channel-tagged MIDI message at an absolute tick — the shared unit for both
/// SMF export and live output. The stream is sorted by `(tick, order)` for stable,
/// deterministic emission of simultaneous messages.
pub struct TimedMessage {
    pub tick: u64,
    pub order: u64,
    /// Zero-based MIDI channel (0..=15).
    pub channel: u8,
    pub message: MidiMessage,
}

/// Resolves every MIDI track of the project into a single time-ordered stream of
/// channel-tagged messages. Reused by live output so what plays live is identical
/// to what `export` writes to disk (same latency compensation, reset block, etc.).
pub fn resolve_project(
    project: &Project,
    options: &ExportOptions,
) -> Result<Vec<TimedMessage>, String> {
    let mut all: Vec<TimedMessage> = Vec::new();
    let end = song_end(project, options);
    for track in &project.tracks {
        if let Track::Midi(t) = track {
            if !(1..=16).contains(&t.midi_channel) {
                return Err(format!("track '{}': MIDI channel must be 1-16", t.name));
            }
            let def = definition::find(&t.definition_id)?;
            let channel = t.midi_channel - 1;
            for e in resolve_track(t, &def, project.bpm, end, options)? {
                all.push(TimedMessage {
                    tick: e.tick,
                    order: e.order,
                    channel,
                    message: e.message,
                });
            }
        }
    }
    all.sort_by_key(|m| (m.tick, m.order));
    Ok(all)
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
    use crate::project::{AutomationCurve, AutomationView, MidiTrack, Project, FORMAT_VERSION};

    fn bp(tick: u64, value: u8) -> Breakpoint {
        Breakpoint {
            tick,
            value,
            shape: Shape::Linear,
            tension: 0.0,
        }
    }

    fn shaped(tick: u64, value: u8, shape: Shape, tension: f32) -> Breakpoint {
        Breakpoint {
            tick,
            value,
            shape,
            tension,
        }
    }

    fn curve(command_id: &str, resolution_ms: u32, breakpoints: Vec<Breakpoint>) -> AutomationCurve {
        AutomationCurve {
            command_id: command_id.into(),
            enabled: true,
            resolution_ms,
            resend_ms: 0,
            breakpoints,
        }
    }

    /// A bare MIDI track carrying only automation, for the curve tests.
    fn curve_track(definition_id: &str, curves: Vec<AutomationCurve>) -> MidiTrack {
        MidiTrack {
            name: "t".into(),
            definition_id: definition_id.into(),
            midi_channel: 1,
            latency_ms: 0,
            mute: false,
            solo: false,
            events: vec![],
            automation: curves,
            automation_view: AutomationView::default(),
        }
    }

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
                    command_id: "delay-hold".into(),
                    tick: 3840, // bar 2
                    length: 3840,
                    params: HashMap::new(),
                    lane: 1,
                },
            ],
            // Volume ramp 0 -> 127 across bar 3; resolution 5 ms → 10 ticks/step
            // @120 BPM, i.e. full 7-bit detail.
            automation: vec![curve("volume-pedal", 5, vec![bp(7680, 0), bp(9600, 127)])],
            automation_view: AutomationView::default(),
        };
        let project = Project {
            format_version: FORMAT_VERSION,
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
    fn tempo_and_signature_follow_project() {
        // Guards against regressions where export ignores project tempo/signature
        // and always emits 120 BPM 4/4 (see issue #3).
        let (mut project, track) = test_project();
        project.bpm = 90.0;
        project.time_signature = (3, 4);
        let bytes = track_to_smf(&project, &track, &ExportOptions::default()).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        let tempo = smf.tracks[0].iter().find_map(|e| match e.kind {
            TrackEventKind::Meta(MetaMessage::Tempo(t)) => Some(t.as_int()),
            _ => None,
        });
        let sig = smf.tracks[0].iter().find_map(|e| match e.kind {
            TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => Some((n, d)),
            _ => None,
        });
        assert_eq!(tempo, Some(666_667), "tempo must follow project.bpm (90 BPM)");
        assert_eq!(sig, Some((3, 2)), "signature must follow project (3/4 -> num=3, dd=2)");
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

        // Reset block at tick 0: disengage of delay-hold (CC35=0) is present
        // before anything else.
        let at_zero: Vec<_> = events.iter().filter(|(t, _, _)| *t == 0).collect();
        assert!(at_zero.len() >= 2); // reset CC35 + preselect CC47
        assert!(matches!(
            at_zero[0].2,
            MidiMessage::Controller { controller, value }
            if controller.as_int() == 35 && value.as_int() == 0
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

        // Hold: engage CC35=1 at bar 2 (tick 3840),
        // disengage CC35=0 at bar 3 (tick 7680)
        assert!(events.iter().any(|(t, _, m)| *t == 3840 && matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 35 && value.as_int() == 1
        )));
        assert!(events.iter().any(|(t, _, m)| *t == 7680 && matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 35 && value.as_int() == 0
        )));

        // Automation: the curve's first value is in effect from the song start,
        // then ramps 0 -> 127 between ticks 7680 and 9600.
        let ramp: Vec<_> = events
            .iter()
            .filter_map(|(t, _, m)| match m {
                MidiMessage::Controller { controller, value } if controller.as_int() == 7 => {
                    Some((*t, value.as_int()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(ramp.first().unwrap(), &(0, 0), "first value at the song start");
        // nothing between the song start and the first breakpoint: the value is flat
        assert!(!ramp.iter().any(|(t, _)| *t > 0 && *t < 7680));
        assert_eq!(ramp.last().unwrap(), &(9600, 127));
        assert!(ramp.windows(2).all(|w| w[0].1 < w[1].1 && w[0].0 < w[1].0));
        // dense enough to be smooth, sparse enough not to flood
        assert!(ramp.len() > 60 && ramp.len() <= 129);
    }

    #[test]
    fn latency_compensation_shifts_messages_early() {
        let (mut project, mut track) = test_project();
        // 100 ms at 120 BPM = 0.1 * 2 beats/s = 0.2 beats = 192 ticks
        track.latency_ms = 100;
        project.tracks = vec![Track::Midi(track.clone())];
        let bytes = track_to_smf(&project, &track, &ExportOptions { reset_block: false, song_end_ticks: 0 }).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        let events = abs_events(&smf);
        // load-slot-2 (CC51) was placed at tick 10 -> shifted to 0 (clamped)
        assert!(events.iter().any(|(t, _, m)| *t == 0 && matches!(
            m, MidiMessage::Controller { controller, .. } if controller.as_int() == 51
        )));
        // delay-hold engage (CC35=1) was at 3840 -> now 3840 - 192 = 3648
        assert!(events.iter().any(|(t, _, m)| *t == 3648 && matches!(
            m, MidiMessage::Controller { controller, value }
            if controller.as_int() == 35 && value.as_int() == 1
        )));
    }

    #[test]
    fn reset_block_can_be_disabled() {
        let (project, track) = test_project();
        let bytes =
            track_to_smf(&project, &track, &ExportOptions { reset_block: false, song_end_ticks: 0 }).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        let events = abs_events(&smf);
        let at_zero: Vec<_> = events.iter().filter(|(t, _, _)| *t == 0).collect();
        // the preselect CC47 and the curve's first value on CC7 — no reset CC35
        assert_eq!(at_zero.len(), 2);
        assert!(!at_zero.iter().any(|(_, _, m)| matches!(
            m, MidiMessage::Controller { controller, .. } if controller.as_int() == 35
        )));
    }

    #[test]
    fn tick_zero_order_is_reset_block_then_events_then_automation() {
        let (project, track) = test_project();
        let bytes = track_to_smf(&project, &track, &ExportOptions::default()).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        let controllers: Vec<u8> = abs_events(&smf)
            .iter()
            .filter(|(t, _, _)| *t == 0)
            .filter_map(|(_, _, m)| match m {
                MidiMessage::Controller { controller, .. } => Some(controller.as_int()),
                _ => None,
            })
            .collect();
        // 35 = Hold disengage (reset block), 47 = preselect (event), 7 = curve
        assert_eq!(controllers, vec![35, 47, 7]);
    }

    #[test]
    fn automation_resolution_controls_step_density() {
        // Same 0->127 ramp, exported at a fine vs. coarse resolution: the
        // coarse one must emit noticeably fewer CC steps.
        fn ramp_count(resolution_ms: u32) -> usize {
            let track = curve_track(
                "kemper-profiler-stage",
                vec![curve("volume-pedal", resolution_ms, vec![bp(0, 0), bp(1920, 127)])],
            );
            let project = Project {
                format_version: FORMAT_VERSION,
                name: "R".into(),
                bpm: 120.0,
                time_signature: (4, 4),
                tracks: vec![Track::Midi(track.clone())],
            };
            let bytes =
                track_to_smf(&project, &track, &ExportOptions { reset_block: false, song_end_ticks: 0 }).unwrap();
            let smf = Smf::parse(&bytes).unwrap();
            abs_events(&smf)
                .iter()
                .filter(|(_, _, m)| {
                    matches!(m, MidiMessage::Controller { controller, .. } if controller.as_int() == 7)
                })
                .count()
        }
        let fine = ramp_count(5); // ~10 ticks/step @120 BPM, full 7-bit detail
        let coarse = ramp_count(50); // ~96 ticks/step, aggressively thinned
        assert!(coarse < fine, "coarse {coarse} should be sparser than fine {fine}");
        assert!(coarse < 30, "coarse resolution should thin hard, got {coarse}");
    }

    /// Kemper CC7 (volume pedal) — continuous, full 0..127 range.
    fn render_kemper_curve(c: AutomationCurve) -> Vec<(u64, u8)> {
        let def = definition::find("kemper-profiler-stage").unwrap();
        let track = curve_track("kemper-profiler-stage", vec![c]);
        let out =
            resolve_track(&track, &def, 120.0, 0, &ExportOptions { reset_block: false, song_end_ticks: 0 }).unwrap();
        cc_values(&out, 7)
    }

    #[test]
    fn first_value_is_written_at_the_song_start() {
        // One point deep into the song: the knob is parked on its value from tick 0.
        let vals = render_kemper_curve(curve("volume-pedal", 10, vec![bp(19_200, 80)]));
        assert_eq!(vals, vec![(0, 80)]);
    }

    #[test]
    fn nothing_is_emitted_after_the_last_breakpoint() {
        let vals = render_kemper_curve(curve("volume-pedal", 10, vec![bp(0, 0), bp(960, 64)]));
        assert_eq!(vals.last().unwrap(), &(960, 64));
        assert!(!vals.iter().any(|(t, _)| *t > 960));
    }

    #[test]
    fn hold_segment_emits_one_step_at_the_next_point() {
        let vals = render_kemper_curve(curve(
            "volume-pedal",
            10,
            vec![bp(0, 20), shaped(960, 100, Shape::Hold, 0.0)],
        ));
        assert_eq!(vals, vec![(0, 20), (960, 100)], "a hold jumps, it does not ramp");
    }

    #[test]
    fn curve_segment_bows_away_from_the_straight_line() {
        let bowed = render_kemper_curve(curve(
            "volume-pedal",
            5,
            vec![bp(0, 0), shaped(960, 100, Shape::Curve, 1.0)],
        ));
        let straight = render_kemper_curve(curve("volume-pedal", 5, vec![bp(0, 0), bp(960, 100)]));
        let at = |vals: &[(u64, u8)], tick: u64| {
            vals.iter().filter(|(t, _)| *t <= tick).last().unwrap().1
        };
        // tension 1.0 → p = 4: halfway through, 0.5^4 * 100 ≈ 6 against a linear 50
        assert!(at(&bowed, 480) <= 10, "got {}", at(&bowed, 480));
        assert!((45..=55).contains(&at(&straight, 480)), "got {}", at(&straight, 480));
        // both still arrive at the same place
        assert_eq!(bowed.last().unwrap().1, 100);
        assert_eq!(straight.last().unwrap().1, 100);
    }

    #[test]
    fn resend_repeats_a_constant_value_across_the_song() {
        // "Park the pedal at 80" plus re-send every 500 ms: one message at the song
        // start, then a duplicate every 500 ms so a dropped CC cannot strand the device.
        let mut c = curve("volume-pedal", 10, vec![bp(0, 80)]);
        c.resend_ms = 500; // @120 BPM = 960 ticks
        let def = definition::find("kemper-profiler-stage").unwrap();
        let track = curve_track("kemper-profiler-stage", vec![c]);
        let out = resolve_track(
            &track,
            &def,
            120.0,
            3840, // one 4/4 bar of song
            &ExportOptions { reset_block: false, song_end_ticks: 3840 },
        )
        .unwrap();
        let vals = cc_values(&out, 7);
        assert_eq!(vals, vec![(0, 80), (960, 80), (1920, 80), (2880, 80)]);
    }

    #[test]
    fn resend_fills_the_gaps_of_a_moving_curve_without_changing_it() {
        let mut c = curve("volume-pedal", 10, vec![bp(0, 0), bp(240, 10), bp(3840, 10)]);
        c.resend_ms = 500;
        let def = definition::find("kemper-profiler-stage").unwrap();
        let track = curve_track("kemper-profiler-stage", vec![c.clone()]);
        let plain = {
            let mut c2 = c.clone();
            c2.resend_ms = 0;
            resolve_track(
                &curve_track("kemper-profiler-stage", vec![c2]),
                &def,
                120.0,
                3840,
                &ExportOptions { reset_block: false, song_end_ticks: 3840 },
            )
            .unwrap()
        };
        let out = resolve_track(
            &track,
            &def,
            120.0,
            3840,
            &ExportOptions { reset_block: false, song_end_ticks: 3840 },
        )
        .unwrap();
        let with = cc_values(&out, 7);
        let without = cc_values(&plain, 7);
        assert!(with.len() > without.len(), "re-sends must add messages");
        // every original message survives, and nothing new introduces a new value
        for m in &without {
            assert!(with.contains(m), "lost {m:?}");
        }
        let long_gap = with.windows(2).any(|w| w[1].0 - w[0].0 > 960);
        assert!(!long_gap, "no gap may exceed the re-send interval: {with:?}");
    }

    #[test]
    fn a_target_can_declare_its_own_messages() {
        // QLC+ selects songs with a Program Change, not a CC — and only jumps.
        let def = definition::find("qlcplus-horizont-hilo").unwrap();
        let track = curve_track(
            "qlcplus-horizont-hilo",
            vec![curve("select-song", 10, vec![bp(0, 0), bp(1920, 4)])],
        );
        let out = resolve_track(
            &track,
            &def,
            120.0,
            0,
            &ExportOptions { reset_block: false, song_end_ticks: 0 },
        )
        .unwrap();
        let programs: Vec<(u64, u8)> = out
            .iter()
            .filter_map(|e| match e.message {
                MidiMessage::ProgramChange { program } => Some((e.tick, program.as_int())),
                _ => None,
            })
            .collect();
        // song 1 from the start, song 5 from bar 2 — and nothing in between
        assert_eq!(programs, vec![(0, 0), (1920, 4)]);
        assert!(
            !out.iter().any(|e| matches!(e.message, MidiMessage::Controller { .. })),
            "a Program Change target must not emit CCs"
        );
    }

    #[test]
    fn a_definition_can_restrict_the_curve_type() {
        let def = definition::find("qlcplus-horizont-hilo").unwrap();
        let Command::Automation(cmd) = find_command(&def, "select-song").unwrap() else {
            panic!("select-song should be an Automation")
        };
        assert_eq!(cmd.target.allowed_shapes(), vec![crate::project::Shape::Hold]);
        assert_eq!(cmd.target.forced_shape(), Some(crate::project::Shape::Hold));

        // A stored Linear is ignored: the export still jumps.
        let track = curve_track(
            "qlcplus-horizont-hilo",
            vec![curve(
                "select-song",
                10,
                vec![shaped(0, 0, Shape::Linear, 0.0), shaped(960, 10, Shape::Linear, 0.0)],
            )],
        );
        let out = resolve_track(
            &track,
            &def,
            120.0,
            0,
            &ExportOptions { reset_block: false, song_end_ticks: 0 },
        )
        .unwrap();
        let programs: Vec<u8> = out
            .iter()
            .filter_map(|e| match e.message {
                MidiMessage::ProgramChange { program } => Some(program.as_int()),
                _ => None,
            })
            .collect();
        assert_eq!(programs, vec![0, 10], "no intermediate songs may be selected");
    }

    #[test]
    fn disabled_or_empty_curves_emit_nothing() {
        let mut off = curve("volume-pedal", 10, vec![bp(0, 0), bp(960, 127)]);
        off.enabled = false;
        assert!(render_kemper_curve(off).is_empty(), "a disabled curve is skipped");
        assert!(
            render_kemper_curve(curve("volume-pedal", 10, vec![])).is_empty(),
            "a curve with no points sends nothing"
        );
    }

    #[test]
    fn curve_values_are_clamped_to_the_target_range() {
        let xml = r#"<DeviceDefinition id="r" version="1" xmlns="https://rigpilot.app/schemas/device-definition/1">
          <Meta><Manufacturer>M</Manufacturer><Model>D</Model></Meta>
          <Commands>
            <Automation id="narrow" name="Narrow"><Target controller="9" min="10" max="100"/></Automation>
          </Commands>
        </DeviceDefinition>"#;
        let def = definition::parse(xml).unwrap();
        let track = curve_track("r", vec![curve("narrow", 5, vec![bp(0, 0), bp(960, 127)])]);
        let out =
            resolve_track(&track, &def, 120.0, 0, &ExportOptions { reset_block: false, song_end_ticks: 0 }).unwrap();
        let vals = cc_values(&out, 9);
        assert!(
            vals.iter().all(|(_, v)| (10..=100).contains(v)),
            "values must stay inside the declared range, got {vals:?}"
        );
        assert_eq!(vals.first().unwrap().1, 10);
        assert_eq!(vals.last().unwrap().1, 100);
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
        assert_eq!(defs.len(), 6);
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

    fn stepped_def() -> DeviceDefinition {
        let xml = r#"<DeviceDefinition id="x" version="1" xmlns="https://rigpilot.app/schemas/device-definition/1">
          <Meta><Manufacturer>M</Manufacturer><Model>D</Model></Meta>
          <Commands>
            <Automation id="wah-toggle" name="Wah">
              <Target controller="11">
                <Label value="0" short="Off">Off</Label>
                <Label value="127" short="On">On</Label>
              </Target>
            </Automation>
          </Commands>
        </DeviceDefinition>"#;
        definition::parse(xml).unwrap()
    }

    fn cc_values(out: &[Emitted], controller: u8) -> Vec<(u64, u8)> {
        out.iter()
            .filter_map(|e| match e.message {
                MidiMessage::Controller { controller: c, value } if c.as_int() == controller => {
                    Some((e.tick, value.as_int()))
                }
                _ => None,
            })
            .collect()
    }

    fn render_stepped(breakpoints: Vec<Breakpoint>) -> Vec<(u64, u8)> {
        let def = stepped_def();
        let track = curve_track("x", vec![curve("wah-toggle", 10, breakpoints)]);
        let out = resolve_track(&track, &def, 120.0, 0, &ExportOptions { reset_block: false, song_end_ticks: 0 }).unwrap();
        cc_values(&out, 11)
    }

    #[test]
    fn stepped_automation_snaps_and_holds() {
        // 0->0, 64->127, 70->127, 127->127 after snapping; sample-and-hold collapses
        // the trailing repeats. No in-between value (64/70) is ever emitted.
        let vals = render_stepped(vec![bp(0, 0), bp(100, 64), bp(200, 70), bp(300, 127)]);
        assert_eq!(vals, vec![(0, 0), (100, 127)]);
        assert!(
            vals.iter().all(|(_, v)| *v == 0 || *v == 127),
            "stepped automation must only emit the discrete set, got {vals:?}"
        );
    }

    #[test]
    fn stepped_automation_snaps_below_and_above_midpoint() {
        // 63 -> 0 (nearer 0), 64 -> 127 (nearer 127)
        assert_eq!(
            render_stepped(vec![bp(0, 63), bp(100, 64)]),
            vec![(0, 0), (100, 127)]
        );
    }

    #[test]
    fn stepped_automation_ignores_a_stored_linear_shape() {
        // A discrete target never interpolates, whatever the segment says.
        let vals = render_stepped(vec![
            shaped(0, 0, Shape::Linear, 0.0),
            shaped(1000, 127, Shape::Curve, 0.8),
        ]);
        assert_eq!(vals, vec![(0, 0), (1000, 127)]);
    }
}
