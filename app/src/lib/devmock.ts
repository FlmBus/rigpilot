// Browser dev mode: the app normally talks to the Tauri backend (device definitions,
// file dialogs, audio import). In a plain browser there is no backend, so we detect
// that and feed mock data — this lets the UI be developed & screenshotted without
// building/launching the native shell. No effect inside the real Tauri app.
import {
  DEFAULT_AUTOMATION_RESOLUTION_MS,
  PROJECT_FORMAT_VERSION,
  type AutomationCurve,
  type Breakpoint,
  type CommandInfo,
  type DefinitionInfo,
  type Project,
  type Shape,
} from "./types";

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
  range: commandType === "automation" ? [0, 127] : null,
  shapes: commandType === "automation" ? (["linear", "curve", "hold"] as Shape[]) : [],
});

const ON_OFF: CommandInfo["steps"] = [
  { value: 0, text: "Off", short: "Off" },
  { value: 127, text: "On", short: "On" },
];

const bp = (tick: number, value: number, shape: Shape = "linear", tension = 0): Breakpoint => ({
  tick,
  value,
  shape,
  tension,
});

const curve = (commandId: string, breakpoints: Breakpoint[]): AutomationCurve => ({
  commandId,
  enabled: true,
  resolutionMs: DEFAULT_AUTOMATION_RESOLUTION_MS,
  resendMs: 0,
  breakpoints,
});

export const mockDefinitions: DefinitionInfo[] = [
  {
    id: "helix-floor",
    manufacturer: "Line 6",
    model: "Helix Floor",
    description: "Demo device definition (browser dev mode).",
    defaultLatency: 10,
    warnings: [],
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
    warnings: [],
    commands: [
      cmd("scene-a", "Scene A", "hold", "Scenes"),
      cmd("scene-b", "Scene B", "hold", "Scenes"),
      cmd("preset", "Select Preset", "one-shot", "Presets"),
      cmd("gain", "Gain", "automation", "Expression"),
    ],
  },
];

export const mockProject: Project = {
  formatVersion: PROJECT_FORMAT_VERSION,
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
        { kind: "hold", commandId: "stomp-a", tick: 960, length: 1920, lane: 0, params: {} },
        { kind: "one-shot", commandId: "snap-1", tick: 2880, lane: 0, params: {} },
        { kind: "hold", commandId: "boost", tick: 1920, length: 960, lane: 1, params: {} },
        { kind: "one-shot", commandId: "snap-2", tick: 3360, lane: 1, params: {} },
      ],
      automation: [
        curve("volume", [
          bp(0, 40),
          bp(1920, 110, "curve", 0.6),
          bp(3840, 70),
        ]),
        curve("fx-loop", [bp(0, 0, "hold"), bp(1920, 127, "hold"), bp(3840, 0, "hold")]),
      ],
      automationView: { command: "volume", height: 96 },
    },
    {
      type: "midi",
      name: "Quad Cortex",
      definitionId: "quad-cortex",
      midiChannel: 2,
      latencyMs: 8,
      mute: true,
      solo: false,
      events: [],
      automation: [curve("gain", [bp(480, 20), bp(1920, 120), bp(3360, 60)])],
      automationView: { command: null, height: 96 },
    },
  ],
};
