//! Device Definition loading (XML, see definitions/device-definition-1.xsd).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct DeviceDefinition {
    #[serde(rename = "@id")]
    pub id: String,
    #[allow(dead_code)] // parsed for completeness; used once definition updates land
    #[serde(rename = "@version")]
    pub version: String,
    #[serde(rename = "Meta")]
    pub meta: Meta,
    #[serde(rename = "Defaults")]
    pub defaults: Option<Defaults>,
    #[serde(rename = "Init")]
    pub init: Option<MessageList>,
    #[serde(rename = "Commands")]
    pub commands: Commands,
}

#[derive(Debug, Deserialize)]
pub struct Meta {
    #[serde(rename = "Manufacturer")]
    pub manufacturer: String,
    #[serde(rename = "Model")]
    pub model: String,
    #[allow(dead_code)]
    #[serde(rename = "Author")]
    pub author: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "Source", default)]
    pub sources: Vec<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
}

/// Default Device Settings applied when a track selects this definition.
#[derive(Debug, Deserialize)]
pub struct Defaults {
    /// Default latency compensation in milliseconds.
    #[serde(rename = "@latency", default)]
    pub latency: u32,
}

#[derive(Debug, Deserialize)]
pub struct Commands {
    #[serde(rename = "$value", default)]
    pub items: Vec<Command>,
}

#[derive(Debug, Deserialize)]
pub enum Command {
    OneShot(OneShotCommand),
    Hold(HoldCommand),
    Automation(AutomationCommand),
}

impl Command {
    pub fn id(&self) -> &str {
        match self {
            Command::OneShot(c) => &c.id,
            Command::Hold(c) => &c.id,
            Command::Automation(c) => &c.id,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct OneShotCommand {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@short")]
    pub short: Option<String>,
    /// Typical execution latency of this command in ms.
    #[serde(rename = "@latency")]
    pub latency: Option<u32>,
    #[serde(rename = "@group")]
    pub group: Option<String>,
    #[serde(rename = "@description")]
    pub description: Option<String>,
    #[serde(rename = "@deterministic", default = "default_true")]
    pub deterministic: bool,
    #[serde(rename = "$value", default)]
    pub children: Vec<OneShotChild>,
}

impl OneShotCommand {
    pub fn params(&self) -> impl Iterator<Item = &Param> {
        self.children.iter().filter_map(|c| match c {
            OneShotChild::Param(p) => Some(p),
            _ => None,
        })
    }
    pub fn messages(&self) -> impl Iterator<Item = Message> + '_ {
        self.children.iter().filter_map(|c| match c {
            OneShotChild::Param(_) => None,
            OneShotChild::NoteOn(m) => Some(Message::NoteOn(m.clone())),
            OneShotChild::NoteOff(m) => Some(Message::NoteOff(m.clone())),
            OneShotChild::ControlChange(m) => Some(Message::ControlChange(m.clone())),
            OneShotChild::ProgramChange(m) => Some(Message::ProgramChange(m.clone())),
        })
    }
}

#[derive(Debug, Deserialize)]
pub enum OneShotChild {
    Param(Param),
    NoteOn(NoteMsg),
    NoteOff(NoteMsg),
    ControlChange(CcMsg),
    ProgramChange(PcMsg),
}

#[derive(Debug, Deserialize)]
pub struct HoldCommand {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@short")]
    pub short: Option<String>,
    /// Typical execution latency of this command in ms.
    #[serde(rename = "@latency")]
    pub latency: Option<u32>,
    #[serde(rename = "@group")]
    pub group: Option<String>,
    #[serde(rename = "@description")]
    pub description: Option<String>,
    #[serde(rename = "Param", default)]
    pub params: Vec<Param>,
    #[serde(rename = "Engage")]
    pub engage: MessageList,
    #[serde(rename = "Disengage")]
    pub disengage: MessageList,
}

#[derive(Debug, Deserialize)]
pub struct AutomationCommand {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@short")]
    pub short: Option<String>,
    /// Typical execution latency of this command in ms.
    #[serde(rename = "@latency")]
    pub latency: Option<u32>,
    #[serde(rename = "@group")]
    pub group: Option<String>,
    #[serde(rename = "@description")]
    pub description: Option<String>,
    #[serde(rename = "Target")]
    pub target: Target,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    #[serde(rename = "@controller")]
    pub controller: u8,
    #[allow(dead_code)] // used by automation range validation (M4)
    #[serde(rename = "@min", default)]
    pub min: u8,
    #[allow(dead_code)]
    #[serde(rename = "@max", default = "default_max")]
    pub max: u8,
    /// Optional discrete value set. When non-empty the automation is "stepped":
    /// breakpoints snap to these values and export emits a sample-and-hold step
    /// function instead of interpolating.
    #[serde(rename = "Label", default)]
    pub labels: Vec<Label>,
}

impl Target {
    /// Allowed values for a stepped automation, ascending. Empty = continuous.
    pub fn step_values(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self.labels.iter().map(|l| l.value).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Snaps a raw value to the nearest allowed step value (lower on a tie).
    /// Returns the value unchanged when the target is continuous.
    pub fn snap(&self, value: u8) -> u8 {
        let steps = self.step_values();
        if steps.is_empty() {
            return value;
        }
        *steps
            .iter()
            .min_by_key(|s| (value as i16 - **s as i16).unsigned_abs())
            .unwrap()
    }
}

fn default_max() -> u8 {
    127
}

#[derive(Debug, Deserialize)]
pub struct Param {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@min")]
    pub min: u8,
    #[serde(rename = "@max")]
    pub max: u8,
    #[serde(rename = "@default")]
    pub default: Option<u8>,
    #[serde(rename = "Label", default)]
    pub labels: Vec<Label>,
}

#[derive(Debug, Deserialize)]
pub struct Label {
    #[serde(rename = "@value")]
    pub value: u8,
    #[serde(rename = "@short")]
    pub short: Option<String>,
    #[serde(rename = "$text")]
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageList {
    #[serde(rename = "$value", default)]
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum Message {
    NoteOn(NoteMsg),
    NoteOff(NoteMsg),
    ControlChange(CcMsg),
    ProgramChange(PcMsg),
}

fn default_velocity() -> String {
    "127".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct NoteMsg {
    #[serde(rename = "@note")]
    pub note: String,
    #[serde(rename = "@velocity", default = "default_velocity")]
    pub velocity: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CcMsg {
    #[serde(rename = "@controller")]
    pub controller: u8,
    #[serde(rename = "@value")]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PcMsg {
    #[serde(rename = "@program")]
    pub program: String,
}

/// Resolves a message value field: either a literal 0..=127 or a `$param` placeholder.
pub fn resolve_value(raw: &str, params: &HashMap<String, u8>) -> Result<u8, String> {
    if let Some(name) = raw.strip_prefix('$') {
        params
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing value for parameter '{name}'"))
    } else {
        raw.parse::<u8>()
            .ok()
            .filter(|v| *v <= 127)
            .ok_or_else(|| format!("invalid MIDI value '{raw}'"))
    }
}

/// Definitions bundled with the app.
/// TODO(M2+): load from a resource directory + project-imported files instead.
const BUNDLED: &[&str] = &[
    include_str!("../../../definitions/kemper-profiler-stage.xml"),
    include_str!("../../../definitions/neural-dsp-quad-cortex.xml"),
    include_str!("../../../definitions/harley-benton-dnafx-git-pro.xml"),
    include_str!("../../../definitions/fractal-axe-fx-iii.xml"),
    include_str!("../../../definitions/headrush-prime.xml"),
];

pub fn parse(xml: &str) -> Result<DeviceDefinition, String> {
    quick_xml::de::from_str(xml).map_err(|e| format!("invalid definition file: {e}"))
}

pub fn bundled() -> Result<Vec<DeviceDefinition>, String> {
    BUNDLED.iter().map(|xml| parse(xml)).collect()
}

pub fn find(id: &str) -> Result<DeviceDefinition, String> {
    bundled()?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("unknown device definition '{id}'"))
}

// ---------- UI-facing summary (serialized to the frontend) ----------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionInfo {
    pub id: String,
    pub manufacturer: String,
    pub model: String,
    pub description: Option<String>,
    pub default_latency: u32,
    pub commands: Vec<CommandInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub id: String,
    pub name: String,
    pub short: Option<String>,
    pub latency: u32,
    pub command_type: String, // "one-shot" | "hold" | "automation"
    pub group: Option<String>,
    pub description: Option<String>,
    pub deterministic: bool,
    pub params: Vec<ParamInfo>,
    /// Discrete value set for a stepped Automation (empty for continuous
    /// automations and all other command types).
    pub steps: Vec<LabelInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamInfo {
    pub id: String,
    pub name: String,
    pub min: u8,
    pub max: u8,
    pub default: Option<u8>,
    pub labels: Vec<LabelInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelInfo {
    pub value: u8,
    pub text: String,
    pub short: Option<String>,
}

fn param_info(p: &Param) -> ParamInfo {
    ParamInfo {
        id: p.id.clone(),
        name: p.name.clone(),
        min: p.min,
        max: p.max,
        default: p.default,
        labels: p.labels.iter().map(label_info).collect(),
    }
}

fn label_info(l: &Label) -> LabelInfo {
    LabelInfo {
        value: l.value,
        text: l.text.clone(),
        short: l.short.clone(),
    }
}

pub fn info(def: &DeviceDefinition) -> DefinitionInfo {
    let commands = def
        .commands
        .items
        .iter()
        .map(|c| match c {
            Command::OneShot(c) => CommandInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                short: c.short.clone(),
                latency: c.latency.unwrap_or(0),
                command_type: "one-shot".into(),
                group: c.group.clone(),
                description: c.description.clone(),
                deterministic: c.deterministic,
                params: c.params().map(param_info).collect(),
                steps: vec![],
            },
            Command::Hold(c) => CommandInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                short: c.short.clone(),
                latency: c.latency.unwrap_or(0),
                command_type: "hold".into(),
                group: c.group.clone(),
                description: c.description.clone(),
                deterministic: true,
                params: c.params.iter().map(param_info).collect(),
                steps: vec![],
            },
            Command::Automation(c) => CommandInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                short: c.short.clone(),
                latency: c.latency.unwrap_or(0),
                command_type: "automation".into(),
                group: c.group.clone(),
                description: c.description.clone(),
                deterministic: true,
                params: vec![],
                steps: c.target.labels.iter().map(label_info).collect(),
            },
        })
        .collect();

    DefinitionInfo {
        id: def.id.clone(),
        manufacturer: def.meta.manufacturer.clone(),
        model: def.meta.model.clone(),
        description: def.meta.description.clone(),
        default_latency: def.defaults.as_ref().map(|d| d.latency).unwrap_or(0),
        commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_snaps_to_nearest_discrete_value() {
        let xml = r#"<DeviceDefinition id="x" version="1" xmlns="https://rigpilot.app/schemas/device-definition/1">
          <Meta><Manufacturer>M</Manufacturer><Model>D</Model></Meta>
          <Commands>
            <Automation id="a" name="A">
              <Target controller="11">
                <Label value="0">Off</Label>
                <Label value="127">On</Label>
              </Target>
            </Automation>
          </Commands>
        </DeviceDefinition>"#;
        let def = parse(xml).unwrap();
        let Command::Automation(c) = &def.commands.items[0] else {
            panic!("expected automation");
        };
        assert_eq!(c.target.step_values(), vec![0, 127]);
        assert_eq!(c.target.snap(0), 0);
        assert_eq!(c.target.snap(63), 0); // nearer 0
        assert_eq!(c.target.snap(64), 127); // nearer 127
        assert_eq!(c.target.snap(127), 127);
    }

    #[test]
    fn continuous_target_does_not_snap() {
        let xml = r#"<DeviceDefinition id="x" version="1" xmlns="https://rigpilot.app/schemas/device-definition/1">
          <Meta><Manufacturer>M</Manufacturer><Model>D</Model></Meta>
          <Commands>
            <Automation id="a" name="A"><Target controller="7"/></Automation>
          </Commands>
        </DeviceDefinition>"#;
        let def = parse(xml).unwrap();
        let Command::Automation(c) = &def.commands.items[0] else {
            panic!("expected automation");
        };
        assert!(c.target.step_values().is_empty());
        assert_eq!(c.target.snap(42), 42);
    }

    #[test]
    fn automation_steps_exposed_in_info() {
        let xml = r#"<DeviceDefinition id="x" version="1" xmlns="https://rigpilot.app/schemas/device-definition/1">
          <Meta><Manufacturer>M</Manufacturer><Model>D</Model></Meta>
          <Commands>
            <Automation id="a" name="A">
              <Target controller="11">
                <Label value="0" short="Off">Off</Label>
                <Label value="127" short="On">On</Label>
              </Target>
            </Automation>
          </Commands>
        </DeviceDefinition>"#;
        let def = parse(xml).unwrap();
        let cmd = &info(&def).commands[0];
        assert_eq!(cmd.command_type, "automation");
        assert_eq!(cmd.steps.len(), 2);
        assert_eq!(cmd.steps[1].value, 127);
        assert_eq!(cmd.steps[1].short.as_deref(), Some("On"));
    }
}
