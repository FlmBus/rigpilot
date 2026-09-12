//! Live MIDI output: streams a project's resolved MIDI messages to a chosen
//! output port in real time, in sync with the frontend's audio transport.
//!
//! The message stream is produced by `export::resolve_project`, so live output
//! is identical to what Export writes to disk (same latency compensation, reset
//! block, automation stepping). A single output port is used; tracks are kept
//! apart by their MIDI channel, exactly like export. Virtual ports are expected
//! to already exist on the system — they simply appear in `list_midi_ports`.

use crate::export::{self, ExportOptions};
use crate::project::{Project, PPQN};
use midir::{MidiOutput, MidiOutputConnection};
use midly::MidiMessage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// App-managed live-output engine. Holds the currently playing scheduler thread,
/// if any.
#[derive(Default)]
pub struct MidiEngine {
    playback: Mutex<Option<Playback>>,
}

struct Playback {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Encodes a channel + message into raw MIDI bytes. Returns `None` for message
/// kinds RigPilot never emits (aftertouch, pitch bend).
fn to_raw(channel: u8, msg: &MidiMessage) -> Option<Vec<u8>> {
    let ch = channel & 0x0F;
    Some(match msg {
        MidiMessage::NoteOn { key, vel } => vec![0x90 | ch, key.as_int(), vel.as_int()],
        MidiMessage::NoteOff { key, vel } => vec![0x80 | ch, key.as_int(), vel.as_int()],
        MidiMessage::Controller { controller, value } => {
            vec![0xB0 | ch, controller.as_int(), value.as_int()]
        }
        MidiMessage::ProgramChange { program } => vec![0xC0 | ch, program.as_int()],
        _ => return None,
    })
}

/// Sends All Sound Off + All Notes Off on every channel — used on stop/panic so
/// nothing hangs (held notes, stuck CCs' sound).
fn silence(conn: &mut MidiOutputConnection) {
    for ch in 0u8..16 {
        let _ = conn.send(&[0xB0 | ch, 120, 0]); // All Sound Off
        let _ = conn.send(&[0xB0 | ch, 123, 0]); // All Notes Off
    }
}

fn open_port(port_name: &str) -> Result<MidiOutputConnection, String> {
    let out = MidiOutput::new("rigpilot").map_err(|e| e.to_string())?;
    let port = out
        .ports()
        .into_iter()
        .find(|p| out.port_name(p).map(|n| n == port_name).unwrap_or(false))
        .ok_or_else(|| format!("MIDI output port '{port_name}' not found"))?;
    out.connect(&port, "rigpilot-out").map_err(|e| e.to_string())
}

/// Locks the playback slot, recovering from a poisoned mutex (a panicked
/// scheduler thread must not take the whole app down with it).
fn lock_playback(engine: &MidiEngine) -> std::sync::MutexGuard<'_, Option<Playback>> {
    engine
        .playback
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Stops the current playback (if any), signalling the thread to flush + silence.
fn stop_playback(engine: &MidiEngine) {
    if let Some(pb) = lock_playback(engine).take() {
        pb.stop.store(true, Ordering::Relaxed);
        let _ = pb.handle.join();
    }
}

/// Lists available MIDI output ports (hardware + any pre-existing virtual ports).
#[tauri::command]
pub fn list_midi_ports() -> Result<Vec<String>, String> {
    let out = MidiOutput::new("rigpilot").map_err(|e| e.to_string())?;
    Ok(out
        .ports()
        .iter()
        .filter_map(|p| out.port_name(p).ok())
        .collect())
}

/// Starts live playback from `from_seconds`. `start_in_ms` is a shared lead-in so
/// the MIDI scheduler and the frontend audio clock kick off aligned. Messages
/// before the playhead are skipped (mid-song starts don't replay earlier state).
#[tauri::command]
pub fn midi_start(
    engine: tauri::State<MidiEngine>,
    project: Project,
    port_name: String,
    from_seconds: f64,
    start_in_ms: u64,
    options: Option<ExportOptions>,
) -> Result<(), String> {
    let options = options.unwrap_or_default();
    let messages = export::resolve_project(&project, &options)?;

    if !project.bpm.is_finite() || project.bpm <= 0.0 {
        return Err(format!("Invalid project BPM: {}", project.bpm));
    }
    if !from_seconds.is_finite() || from_seconds < 0.0 {
        return Err(format!("Invalid playback position: {from_seconds}"));
    }

    // Precompute the wall-clock schedule before opening the port.
    let secs_per_tick = 60.0 / (project.bpm * PPQN as f64);
    let origin = Instant::now() + Duration::from_millis(start_in_ms);
    let mut schedule: Vec<(Instant, Vec<u8>)> = Vec::with_capacity(messages.len());
    for m in &messages {
        let secs = m.tick as f64 * secs_per_tick;
        if secs + 1e-9 < from_seconds {
            continue; // before the playhead
        }
        if let Some(bytes) = to_raw(m.channel, &m.message) {
            // Clamp: the epsilon in the skip above can leave a tiny negative
            // offset, and Duration::from_secs_f64 aborts on negative input.
            let offset = (secs - from_seconds).max(0.0);
            schedule.push((origin + Duration::from_secs_f64(offset), bytes));
        }
    }

    // Replace any running playback, then open the port and spawn the scheduler.
    stop_playback(&engine);
    let mut conn = open_port(&port_name)?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let handle = std::thread::spawn(move || {
        for (target, bytes) in schedule {
            loop {
                if stop_thread.load(Ordering::Relaxed) {
                    silence(&mut conn);
                    return;
                }
                let now = Instant::now();
                if now >= target {
                    let _ = conn.send(&bytes);
                    break;
                }
                // Sleep in short slices so a stop stays responsive.
                std::thread::sleep((target - now).min(Duration::from_millis(5)));
            }
        }
        // Natural end: leave the device in whatever state the sequence dictated
        // (the sequence's own Disengage/Reset messages already ran).
    });

    *lock_playback(&engine) = Some(Playback { stop, handle });
    Ok(())
}

/// Stops live playback and silences all channels.
#[tauri::command]
pub fn midi_stop(engine: tauri::State<MidiEngine>) {
    stop_playback(&engine);
}

/// Panic: stops playback (if any) and force-silences every channel on the port.
#[tauri::command]
pub fn midi_panic(engine: tauri::State<MidiEngine>, port_name: String) -> Result<(), String> {
    stop_playback(&engine);
    // Also send an explicit silence in case nothing was playing but a note hung.
    if let Ok(mut conn) = open_port(&port_name) {
        silence(&mut conn);
    }
    Ok(())
}
