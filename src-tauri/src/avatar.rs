//! The avatar in the left ear: its definition file and the mood it shows. See CONTRACT.md section 10.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

use crate::media::Media;
use crate::session::{Status, Summary};

pub const MAX_BYTES: usize = 262_144;
pub const DEFAULT: &str = include_str!("../avatar/default.avatar.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    User,
    Default,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Avatar {
    pub source: Source,
    pub definition: Value,
}

pub fn avatar_path() -> PathBuf {
    avatar_path_from(std::env::var_os("NOTCH_AVATAR").as_deref(), std::env::var_os("HOME").as_deref())
}

/// `notch_avatar` (`$NOTCH_AVATAR`) when non-empty, else the file under `home`'s Application Support.
pub fn avatar_path_from(notch_avatar: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf {
    match notch_avatar.filter(|p| !p.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(home.unwrap_or_default()).join("Library/Application Support/notch-hud/avatar.json"),
    }
}

pub fn load(path: &Path) -> Result<Value, String> {
    // Checked before opening: the app loads on the main thread at startup, and opening a FIFO would block it.
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err("no file".into()),
        Err(e) => return Err(e.to_string()),
        Ok(meta) if !meta.is_file() => return Err("not a file".into()),
        Ok(_) => {}
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err(format!("larger than {MAX_BYTES} bytes"));
    }
    parse(&String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())?)
}

/// Accepts what avatar-core 0.1.0's `parseAvatarDefinition` accepts: its JSON Schema plus its semantic rules.
pub fn parse(text: &str) -> Result<Value, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("larger than {MAX_BYTES} bytes"));
    }
    NoDuplicates(String::new())
        .deserialize(&mut serde_json::Deserializer::from_str(text))
        .map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    definition(&value)?;
    Ok(value)
}

/// Walks any JSON and fails on an object that has the same key twice, which `serde_json::Value` would silently merge.
/// Carries the JSON pointer of the value it is at, so the error names the repeated member.
struct NoDuplicates(String);

impl<'de> DeserializeSeed<'de> for NoDuplicates {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for NoDuplicates {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON")
    }
    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut i = 0;
        while seq.next_element_seed(NoDuplicates(child(&self.0, &i.to_string())))?.is_some() {
            i += 1;
        }
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            let at = child(&self.0, &key);
            if !seen.insert(key) {
                return Err(de::Error::custom(format!("{at}: duplicate key")));
            }
            map.next_value_seed(NoDuplicates(at))?;
        }
        Ok(())
    }
}

// ---- the v1 schema and semantic rules, one check per schema node ----

type Check = Result<(), String>;

fn fail(path: &str, message: impl fmt::Display) -> String {
    if path.is_empty() {
        message.to_string()
    } else {
        format!("{path}: {message}")
    }
}

fn child(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn object<'a>(v: &'a Value, path: &str, allowed: &[&str], required: &[&str]) -> Result<&'a Map<String, Value>, String> {
    let map = v.as_object().ok_or_else(|| fail(path, "must be an object"))?;
    if let Some(missing) = required.iter().find(|k| !map.contains_key(**k)) {
        return Err(fail(&child(path, missing), "is required"));
    }
    if let Some(extra) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(fail(&child(path, extra), "is not allowed"));
    }
    Ok(map)
}

fn number(v: &Value, path: &str, min: f64, max: f64) -> Check {
    match v.as_f64() {
        Some(n) if (min..=max).contains(&n) => Ok(()),
        Some(_) => Err(fail(path, format!("must be from {min} to {max}"))),
        None => Err(fail(path, "must be a number")),
    }
}

fn optional(map: &Map<String, Value>, path: &str, key: &str, check: impl Fn(&Value, &str) -> Check) -> Check {
    map.get(key).map_or(Ok(()), |v| check(v, &child(path, key)))
}

fn text(v: &Value, path: &str, max: usize) -> Check {
    let s = v.as_str().ok_or_else(|| fail(path, "must be a string"))?;
    if s.chars().count() > max {
        return Err(fail(path, format!("must have at most {max} characters")));
    }
    Ok(())
}

fn one_of(v: &Value, path: &str, options: &[&str]) -> Check {
    match v.as_str() {
        Some(s) if options.contains(&s) => Ok(()),
        _ => Err(fail(path, format!("must be one of {}", options.join(", ")))),
    }
}

/// `^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$`, at most 64 characters.
fn is_key(s: &str) -> bool {
    let mut parts = s.split('-');
    let first = parts.next().unwrap_or_default();
    s.len() <= 64
        && first.starts_with(|c: char| c.is_ascii_lowercase())
        && first.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

fn key(v: &Value, path: &str) -> Check {
    match v.as_str() {
        Some(s) if is_key(s) => Ok(()),
        _ => Err(fail(path, "must be a semantic key (lowercase words joined by '-', at most 64 characters)")),
    }
}

fn color(v: &Value, path: &str) -> Check {
    match v.as_str() {
        Some(s) if s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) => {
            Ok(())
        }
        _ => Err(fail(path, "must be a color like #a1b2c3")),
    }
}

fn keyed<'a>(v: &'a Value, path: &str, min: usize, max: usize) -> Result<&'a Map<String, Value>, String> {
    let map = v.as_object().ok_or_else(|| fail(path, "must be an object"))?;
    if map.len() < min || map.len() > max {
        return Err(fail(path, format!("must have {min} to {max} members")));
    }
    if let Some(bad) = map.keys().find(|k| !is_key(k)) {
        return Err(fail(&child(path, bad), "is not a semantic key"));
    }
    Ok(map)
}

fn order<'a>(v: &'a Value, path: &str, min: usize, max: usize) -> Result<Vec<&'a str>, String> {
    let items = v.as_array().ok_or_else(|| fail(path, "must be an array"))?;
    if items.len() < min || items.len() > max {
        return Err(fail(path, format!("must have {min} to {max} entries")));
    }
    let mut seen = HashSet::new();
    for (i, item) in items.iter().enumerate() {
        let at = child(path, &i.to_string());
        key(item, &at)?;
        if !seen.insert(item.as_str().unwrap_or_default()) {
            return Err(fail(&at, "is a duplicate"));
        }
    }
    Ok(items.iter().filter_map(Value::as_str).collect())
}

fn surface(v: &Value, path: &str, types: &[&str]) -> Check {
    let all = ["type", "width", "height", "depth", "roundness", "morphRoundness", "tipRoundness", "baseRoundness"];
    let map = object(v, path, &all, &all[..5])?;
    one_of(&map["type"], &child(path, "type"), types)?;
    for k in ["width", "height", "depth"] {
        number(&map[k], &child(path, k), 0.001, 10000.0)?;
    }
    for k in ["roundness", "morphRoundness", "tipRoundness", "baseRoundness"] {
        optional(map, path, k, |v, p| number(v, p, 0.0, 1.0))?;
    }
    Ok(())
}

fn triple(v: &Value, path: &str, limit: f64) -> Check {
    let items = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| fail(path, "must be 3 numbers"))?;
    items.iter().enumerate().try_for_each(|(i, n)| number(n, &child(path, &i.to_string()), -limit, limit))
}

const SHAPES: [&str; 8] = ["sphere", "mickey", "cursor", "cube", "capsule", "cylinder", "cone", "diamond"];
const NODE_SHAPES: [&str; 6] = ["sphere", "cube", "capsule", "cylinder", "cone", "diamond"];

fn body(v: &Value, path: &str) -> Check {
    let map = object(v, path, &["primary", "nodes"], &["primary", "nodes"])?;
    surface(&map["primary"], &child(path, "primary"), &SHAPES)?;
    let nodes_path = child(path, "nodes");
    let nodes = map["nodes"].as_array().ok_or_else(|| fail(&nodes_path, "must be an array"))?;
    if nodes.len() > 16 {
        return Err(fail(&nodes_path, "must have at most 16 nodes"));
    }
    for (i, node) in nodes.iter().enumerate() {
        let at = child(&nodes_path, &i.to_string());
        let all = ["surface", "position", "rotation"];
        let node = object(node, &at, &all, &all)?;
        surface(&node["surface"], &child(&at, "surface"), &NODE_SHAPES)?;
        triple(&node["position"], &child(&at, "position"), 10000.0)?;
        triple(&node["rotation"], &child(&at, "rotation"), 360.0)?;
    }
    Ok(())
}

fn colors(v: &Value, path: &str, required: &[&str]) -> Check {
    let map = object(v, path, &["body", "eyes"], required)?;
    if map.is_empty() {
        return Err(fail(path, "must have body or eyes"));
    }
    map.iter().try_for_each(|(k, c)| color(c, &child(path, k)))
}

fn bounded(map: &Map<String, Value>, path: &str, keys: &[&str]) -> Check {
    keys.iter().try_for_each(|k| number(&map[*k], &child(path, k), -10000.0, 10000.0))
}

fn expression(v: &Value, path: &str) -> Check {
    let map = object(v, path, &["head", "eyes", "perspective", "motion", "colors"], &["head", "eyes", "perspective", "motion"])?;
    let head = child(path, "head");
    bounded(object(&map["head"], &head, &["x", "y", "z"], &["x", "y", "z"])?, &head, &["x", "y", "z"])?;
    let eyes_path = child(path, "eyes");
    let eyes = object(&map["eyes"], &eyes_path, &["left", "right", "spacing"], &["left", "right", "spacing"])?;
    for side in ["left", "right"] {
        let at = child(&eyes_path, side);
        let fields = ["width", "height", "x", "y", "angle"];
        bounded(object(&eyes[side], &at, &fields, &fields)?, &at, &fields)?;
    }
    bounded(eyes, &eyes_path, &["spacing"])?;
    number(&map["perspective"], &child(path, "perspective"), 0.1, 10.0)?;
    let motion_path = child(path, "motion");
    let motion = object(&map["motion"], &motion_path, &["eyes", "body"], &["eyes", "body"])?;
    one_of(&motion["eyes"], &child(&motion_path, "eyes"), &["none", "microSaccades", "shake"])?;
    one_of(&motion["body"], &child(&motion_path, "body"), &["none", "slowDrift", "shake"])?;
    optional(map, path, "colors", |v, p| colors(v, p, &[]))
}

fn animation(v: &Value, path: &str, expressions: &Map<String, Value>) -> Check {
    let map = object(v, path, &["playbackMode", "steps", "blink", "metadata"], &["playbackMode", "steps", "blink"])?;
    one_of(&map["playbackMode"], &child(path, "playbackMode"), &["loop", "once", "pingPong"])?;
    let steps_path = child(path, "steps");
    let steps = map["steps"].as_array().ok_or_else(|| fail(&steps_path, "must be an array"))?;
    if steps.is_empty() || steps.len() > 128 {
        return Err(fail(&steps_path, "must have 1 to 128 steps"));
    }
    for (i, step) in steps.iter().enumerate() {
        let at = child(&steps_path, &i.to_string());
        let all = ["expression", "holdMs", "transitionMs", "transition"];
        let step = object(step, &at, &all, &all)?;
        let expr_path = child(&at, "expression");
        key(&step["expression"], &expr_path)?;
        number(&step["holdMs"], &child(&at, "holdMs"), 100.0, 60000.0)?;
        number(&step["transitionMs"], &child(&at, "transitionMs"), 0.0, 5000.0)?;
        one_of(&step["transition"], &child(&at, "transition"), &["spring", "smooth", "snappy"])?;
        let name = step["expression"].as_str().unwrap_or_default();
        if !expressions.contains_key(name) {
            return Err(fail(&expr_path, format!("unknown expression '{name}'")));
        }
    }
    let blink_path = child(path, "blink");
    let all = ["enabled", "initialDelayMs", "minIntervalMs", "maxIntervalMs", "durationMs"];
    let blink = object(&map["blink"], &blink_path, &all, &all)?;
    if !blink["enabled"].is_boolean() {
        return Err(fail(&child(&blink_path, "enabled"), "must be true or false"));
    }
    number(&blink["initialDelayMs"], &child(&blink_path, "initialDelayMs"), 0.0, 60000.0)?;
    number(&blink["minIntervalMs"], &child(&blink_path, "minIntervalMs"), 250.0, 120000.0)?;
    number(&blink["maxIntervalMs"], &child(&blink_path, "maxIntervalMs"), 250.0, 120000.0)?;
    number(&blink["durationMs"], &child(&blink_path, "durationMs"), 50.0, 2000.0)?;
    if blink["minIntervalMs"].as_f64() > blink["maxIntervalMs"].as_f64() {
        return Err(fail(&child(&blink_path, "minIntervalMs"), "must not be more than maxIntervalMs"));
    }
    optional(map, path, "metadata", |v, p| {
        let meta = object(v, p, &["label", "description", "group"], &[])?;
        if meta.is_empty() {
            return Err(fail(p, "must have label, description or group"));
        }
        optional(meta, p, "label", |v, p| text(v, p, 120))?;
        optional(meta, p, "description", |v, p| text(v, p, 512))?;
        optional(meta, p, "group", |v, p| text(v, p, 64))
    })
}

/// `order` lists exactly the keys of `map`.
fn complete(order: &[&str], map: &Map<String, Value>, path: &str) -> Check {
    if let Some(i) = order.iter().position(|k| !map.contains_key(*k)) {
        return Err(fail(&child(path, &i.to_string()), format!("unknown key '{}'", order[i])));
    }
    if let Some(missing) = map.keys().find(|k| !order.contains(&k.as_str())) {
        return Err(fail(path, format!("is missing key '{missing}'")));
    }
    Ok(())
}

fn definition(v: &Value) -> Check {
    let all = [
        "schema", "schemaVersion", "body", "colors", "expressions", "expressionOrder", "animations", "animationOrder",
        "name", "standardAnimationSet",
    ];
    let map = object(v, "", &all, &all[..8])?;
    if map["schema"].as_str() != Some("bible-strong/avatar-definition") {
        return Err(fail("/schema", "must be \"bible-strong/avatar-definition\""));
    }
    if map["schemaVersion"].as_f64() != Some(1.0) {
        return Err(fail("/schemaVersion", "unsupported version"));
    }
    optional(map, "", "name", |v, p| text(v, p, 120))?;
    optional(map, "", "standardAnimationSet", |v, p| {
        if v.as_f64() == Some(1.0) { Ok(()) } else { Err(fail(p, "must be 1")) }
    })?;
    body(&map["body"], "/body")?;
    colors(&map["colors"], "/colors", &["body", "eyes"])?;
    let expressions = keyed(&map["expressions"], "/expressions", 1, 128)?;
    if !expressions.contains_key("neutral") {
        return Err(fail("/expressions/neutral", "is required"));
    }
    expressions.iter().try_for_each(|(k, e)| expression(e, &child("/expressions", k)))?;
    let expression_order = order(&map["expressionOrder"], "/expressionOrder", 1, 128)?;
    let animations = keyed(&map["animations"], "/animations", 0, 64)?;
    animations.iter().try_for_each(|(k, a)| animation(a, &child("/animations", k), expressions))?;
    let animation_order = order(&map["animationOrder"], "/animationOrder", 0, 64)?;
    complete(&expression_order, expressions, "/expressionOrder")?;
    complete(&animation_order, animations, "/animationOrder")?;
    if expression_order.first() != Some(&"neutral") {
        return Err(fail("/expressionOrder/0", "must be 'neutral'"));
    }
    Ok(())
}

// ---- mood ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    Alert,
    Error,
    Celebrate,
    Working,
    Listening,
    Idle,
    Drowsy,
    Sleeping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Signals {
    pub needs_you: bool,
    pub error: bool,
    pub working: bool,
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Pose {
    pub mood: Mood,
    pub animations: &'static [&'static str],
    pub still: bool,
}

pub const CELEBRATE_MS: u64 = 4_000;
pub const DROWSY_MS: u64 = 120_000;
pub const SLEEP_MS: u64 = 300_000;

pub fn signals(summary: Option<&Summary>, media: Option<&Media>) -> Signals {
    let has = |wanted: &[Status]| summary.is_some_and(|s| s.rows.iter().any(|r| wanted.contains(&r.status)));
    Signals {
        needs_you: has(&[Status::NeedsPermission, Status::NeedsInput]),
        error: has(&[Status::Error]),
        working: summary.is_some_and(|s| s.working > 0),
        playing: media.and_then(|m| m.track.as_ref()).is_some_and(|t| t.playing),
    }
}

pub fn finished(prev: Option<&Summary>, next: &Summary) -> bool {
    let Some(prev) = prev else { return false };
    next.rows.iter().any(|r| {
        r.status == Status::Done && prev.rows.iter().any(|p| p.label == r.label && p.status != Status::Done)
    })
}

pub fn pose(mood: Mood) -> Pose {
    let animations: &'static [&'static str] = match mood {
        Mood::Alert => &["surprised", "curious"],
        Mood::Error => &["sad", "angry"],
        Mood::Celebrate => &["celebrate", "happy", "excited"],
        Mood::Working => &["working", "thinking"],
        Mood::Listening => &["listening"],
        Mood::Idle => &["idle"],
        Mood::Drowsy => &["drowsy", "bored"],
        Mood::Sleeping => &["sleeping"],
    };
    Pose { mood, animations, still: mood == Mood::Sleeping }
}

pub struct Moods {
    celebrate_until: Option<u64>,
    quiet_since: Option<u64>,
    now: u64,
}

impl Moods {
    pub fn new(now_ms: u64) -> Moods {
        Moods { celebrate_until: None, quiet_since: Some(now_ms), now: now_ms }
    }

    pub fn step(&mut self, signals: Signals, finished: bool, now_ms: u64) -> Mood {
        self.now = now_ms;
        if finished {
            self.celebrate_until = Some(now_ms + CELEBRATE_MS);
        }
        let celebrating = self.celebrate_until.is_some_and(|until| now_ms < until);
        let busy = [
            (signals.needs_you, Mood::Alert),
            (signals.error, Mood::Error),
            (celebrating, Mood::Celebrate),
            (signals.working, Mood::Working),
            (signals.playing, Mood::Listening),
        ];
        if let Some(&(_, mood)) = busy.iter().find(|(on, _)| *on) {
            self.quiet_since = None;
            return mood;
        }
        let since = *self.quiet_since.get_or_insert(now_ms);
        match now_ms.saturating_sub(since) {
            q if q < DROWSY_MS => Mood::Idle,
            q if q < SLEEP_MS => Mood::Drowsy,
            _ => Mood::Sleeping,
        }
    }

    pub fn next_change(&self) -> Option<u64> {
        let quiet = self.quiet_since.map(|since| [since + DROWSY_MS, since + SLEEP_MS]);
        self.celebrate_until.into_iter().chain(quiet.into_iter().flatten()).filter(|&at| at > self.now).min()
    }
}


#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
