// Browser dev mode: the app normally talks to the Tauri backend (device definitions,
// file dialogs, audio import). In a plain browser there is no backend, so we detect
// that and feed mock data — this lets the UI be developed & screenshotted without
// building/launching the native shell. No effect inside the real Tauri app.
import type { CommandInfo, DefinitionInfo, Project } from "./types";

export const IS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const cmd = (
  id: string,
  name: string,
  commandType: CommandInfo["commandType"],
  group: string,
  params: CommandInfo["params"] = [],
  steps: CommandInfo["steps"] = [],
): CommandInfo => ({
  id,
  name,
  short: null,
  latency: 0,
  commandType,
  group,
  description: `${name} — demo command (browser dev mode).`,
  deterministic: true,
  params,
  steps,
});

const ON_OFF: CommandInfo["steps"] = [
  { value: 0, text: "Off", short: "Off" },
  { value: 127, text: "On", short: "On" },
];

export const mockDefinitions: DefinitionInfo[] = [
  {
    id: "helix-floor",
    manufacturer: "Line 6",
    model: "Helix Floor",
    description: "Demo device definition (browser dev mode).",
    defaultLatency: 10,
    commands: [
      cmd("stomp-a", "Stomp A", "hold", "Stomps"),
      cmd("stomp-b", "Stomp B", "hold", "Stomps"),
      cmd("boost", "Boost", "hold", "Stomps"),
      cmd("drive", "Drive", "hold", "Stomps"),
      cmd("snap-1", "Snap 1", "one-shot", "Snapshots"),
      cmd("snap-2", "Snap 2", "one-shot", "Snapshots"),
      cmd("snap-3", "Snap 3", "one-shot", "Snapshots"),
      cmd("volume", "Volume", "automation", "Expression"),
      cmd("wah", "Wah", "automation", "Expression"),
      cmd("pitch", "Pitch", "automation", "Expression"),
      cmd("fx-loop", "FX Loop (stepped)", "automation", "Expression", [], ON_OFF),
    ],
  },
  {
    id: "quad-cortex",
    manufacturer: "Neural DSP",
    model: "Quad Cortex",
    description: "Demo device definition (browser dev mode).",
    defaultLatency: 8,
    commands: [
      cmd("scene-a", "Scene A", "hold", "Scenes"),
      cmd("scene-b", "Scene B", "hold", "Scenes"),
      cmd("preset", "Select Preset", "one-shot", "Presets"),
      cmd("gain", "Gain", "automation", "Expression"),
    ],
  },
];

export const mockProject: Project = {
  name: "Demo Song",
  bpm: 120,
  timeSignature: [4, 4],
  tracks: [
    {
      type: "audio",
      name: "Reference Mix",
      file: "song.wav",
      volume: 1,
      pan: 0,
      mute: false,
      solo: false,
      offsetTicks: 0,
      waveformGain: 1,
    },
    {
      type: "midi",
      name: "Helix Floor",
      definitionId: "helix-floor",
      midiChannel: 1,
      latencyMs: 10,
      mute: false,
      solo: false,
      events: [
        { kind: "automation", commandId: "volume", tick: 0, length: 3840, lane: 0, breakpoints: [[0, 40], [1920, 110], [3840, 70]], params: {} },
        { kind: "hold", commandId: "stomp-a", tick: 960, length: 1920, lane: 1, params: {} },
        { kind: "one-shot", commandId: "snap-1", tick: 2880, lane: 1, params: {} },
        { kind: "hold", commandId: "boost", tick: 1920, length: 960, lane: 2, params: {} },
        { kind: "one-shot", commandId: "snap-2", tick: 3360, lane: 2, params: {} },
        { kind: "automation", commandId: "fx-loop", tick: 0, length: 3840, lane: 3, breakpoints: [[0, 0], [1920, 127], [3840, 0]], params: {} },
      ],
    },
    {
      type: "midi",
      name: "Quad Cortex",
      definitionId: "quad-cortex",
      midiChannel: 2,
      latencyMs: 8,
      mute: true,
      solo: false,
      events: [
        { kind: "automation", commandId: "gain", tick: 480, length: 2880, lane: 0, breakpoints: [[0, 20], [1440, 120], [2880, 60]], params: {} },
      ],
    },
  ],
};
