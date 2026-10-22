use super::*;
use crate::media::{Health, Media, Track, Via};
use crate::session::{Row, Status, Summary};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

// Every expected value below is written from CONTRACT.md section 10, not read from avatar.rs.

const T: u64 = 1_800_000_000_000;

// --- a valid definition and single-rule mutations --------------------------------------------------------------

fn expr(x: f64) -> Value {
    json!({
        "head": { "x": 0, "y": 0, "z": 0 },
        "eyes": {
            "left": { "width": 10, "height": 10, "x": -20.0 + x, "y": 0, "angle": 0 },
            "right": { "width": 10, "height": 10, "x": 20, "y": 0, "angle": 0 },
            "spacing": 0
        },
        "perspective": 1,
        "motion": { "eyes": "none", "body": "none" }
    })
}

fn animation() -> Value {
    json!({
        "playbackMode": "loop",
        "steps": [{ "expression": "calm", "holdMs": 500, "transitionMs": 100, "transition": "smooth" }],
        "blink": { "enabled": false, "initialDelayMs": 0, "minIntervalMs": 250, "maxIntervalMs": 250, "durationMs": 50 }
    })
}

fn base() -> Value {
    json!({
        "schema": "bible-strong/avatar-definition",
        "schemaVersion": 1,
        "name": "ball",
        "body": { "primary": { "type": "sphere", "width": 100, "height": 100, "depth": 100, "roundness": 1 }, "nodes": [] },
        "colors": { "body": "#ffffff", "eyes": "#000000" },
        "expressions": { "neutral": expr(0.0), "calm": expr(1.0) },
        "expressionOrder": ["neutral", "calm"],
        "animations": { "idle": animation() },
        "animationOrder": ["idle"]
    })
}

fn text(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

// `base()` with the member at the JSON pointer replaced.
fn with(path: &str, value: Value) -> Value {
    let mut v = base();
    *v.pointer_mut(path).unwrap_or_else(|| panic!("no member at {path}")) = value;
    v
}

// `base()` with `value` added as member `key` of the object at `parent` ("" is the root).
fn plus(parent: &str, key: &str, value: Value) -> Value {
    let mut v = base();
    v.pointer_mut(parent).unwrap().as_object_mut().unwrap().insert(key.to_string(), value);
    v
}

fn minus(parent: &str, key: &str) -> Value {
    let mut v = base();
    assert!(v.pointer_mut(parent).unwrap().as_object_mut().unwrap().remove(key).is_some(), "no {key} in {parent}");
    v
}

// Runs every case, then fails once with all the cases the parser got wrong.
fn expect(cases: Vec<(String, String, bool)>) {
    let wrong: Vec<String> = cases
        .into_iter()
        .filter_map(|(name, input, accept)| {
            let got = parse(&input);
            match (accept, &got) {
                (true, Err(e)) => Some(format!("{name}: want Ok, got Err({e})")),
                (false, Ok(_)) => Some(format!("{name}: want Err, got Ok")),
                _ => None,
            }
        })
        .collect();
    assert!(wrong.is_empty(), "{} case(s) wrong:\n{}", wrong.len(), wrong.join("\n"));
}

fn case(name: &str, v: Value, accept: bool) -> (String, String, bool) {
    (name.to_string(), text(&v), accept)
}

// A pointer table: each row sets the member to the value and expects accept or reject.
fn table(rows: &[(&str, Value, bool)]) {
    expect(rows.iter().map(|(p, v, ok)| case(&format!("{p} = {v}"), with(p, v.clone()), *ok)).collect());
}

fn reason_of(input: &str) -> String {
    parse(input).expect_err("want Err")
}

fn key_of(len: usize) -> String {
    let mut k = String::from("a");
    while k.len() < len {
        k.push('b');
    }
    k
}

// --- parse: the base and rule 1 ---------------------------------------------------------------------------------

#[test]
fn parse_accepts_a_valid_definition_and_returns_its_json() {
    let v = base();
    assert_eq!(parse(&text(&v)), Ok(v));
}

#[test]
fn parse_accepts_whitespace_formatting() {
    let pretty = serde_json::to_string_pretty(&base()).unwrap();
    assert_eq!(parse(&pretty), Ok(base()));
}

#[test]
fn parse_rule_1_rejects_invalid_json() {
    expect(vec![
        ("empty".into(), String::new(), false),
        ("whitespace only".into(), "   ".into(), false),
        ("truncated".into(), text(&base())[..40].to_string(), false),
        ("trailing garbage".into(), format!("{} x", text(&base())), false),
        ("two documents".into(), format!("{}{}", text(&base()), text(&base())), false),
        ("a bare number".into(), "1".into(), false),
        ("null".into(), "null".into(), false),
        ("an array".into(), "[]".into(), false),
        ("single quotes".into(), text(&base()).replace('"', "'"), false),
        ("trailing comma".into(), text(&base()).replacen("}}", "},}", 1), false),
    ]);
}

#[test]
fn parse_rule_1_rejects_a_repeated_key_in_any_object() {
    let t = text(&base());
    let calm = "\"calm\":{\"eyes\":{\"left\":{\"angle\":0,\"height\":1,\"width\":1,\"x\":0,\"y\":0},\"right\":{\"angle\":0,\"height\":1,\"width\":1,\"x\":0,\"y\":0},\"spacing\":0},\"head\":{\"x\":0,\"y\":0,\"z\":0},\"motion\":{\"body\":\"none\",\"eyes\":\"none\"},\"perspective\":1}";
    expect(vec![
        ("root".into(), t.replacen("\"schemaVersion\":1", "\"schemaVersion\":1,\"schemaVersion\":1", 1), false),
        ("colors".into(), t.replacen("\"body\":\"#ffffff\"", "\"body\":\"#ffffff\",\"body\":\"#ffffff\"", 1), false),
        ("expressions".into(), t.replacen("\"calm\":{", &format!("{calm},\"calm\":{{"), 1), false),
        ("a step".into(), t.replacen("\"holdMs\":500", "\"holdMs\":500,\"holdMs\":500", 1), false),
        ("the key written with an escape".into(), t.replacen("\"schemaVersion\":1", "\"schemaVersion\":1,\"schema\\u0056ersion\":1", 1), false),
    ]);
}

#[test]
fn parse_rule_1_reports_the_repeated_key_with_its_pointer() {
    let t = text(&base()).replacen("\"body\":\"#ffffff\"", "\"body\":\"#ffffff\",\"body\":\"#ffffff\"", 1);
    let r = reason_of(&t);
    assert!(r.starts_with("/colors/body: "), "reason was {r:?}");
}

#[test]
fn parse_rule_1_size_limit_is_max_bytes_utf8_bytes() {
    assert_eq!(MAX_BYTES, 262_144);
    let t = text(&base());
    let pad = |n: usize| format!("{t}{}", " ".repeat(n - t.len()));
    assert_eq!(pad(MAX_BYTES).len(), MAX_BYTES);
    assert_eq!(parse(&pad(MAX_BYTES)), Ok(base()), "exactly MAX_BYTES");
    assert!(parse(&pad(MAX_BYTES + 1)).is_err(), "one byte over MAX_BYTES");
    assert!(parse(&pad(MAX_BYTES * 2)).is_err(), "twice MAX_BYTES");
}

#[test]
fn parse_rule_1_counts_bytes_not_characters() {
    // 'é' is two bytes, so the name adds bytes beyond its characters; trailing whitespace lands the text on the limit.
    let mut v = base();
    v["name"] = json!("é".repeat(120));
    let t = text(&v);
    assert!(t.len() > t.chars().count());
    let at = format!("{t}{}", " ".repeat(MAX_BYTES - t.len()));
    assert_eq!(at.len(), MAX_BYTES);
    assert_eq!(parse(&at), Ok(v));
    assert!(parse(&format!("{at} ")).is_err());
}

// --- parse: rule 2, the v1 schema -------------------------------------------------------------------------------

#[test]
fn parse_rule_2_top_level_members() {
    let mut cases = vec![
        case("schema is another string", with("/schema", json!("bible-strong/other")), false),
        case("schema is a number", with("/schema", json!(1)), false),
        case("schemaVersion 2", with("/schemaVersion", json!(2)), false),
        case("schemaVersion 0", with("/schemaVersion", json!(0)), false),
        case("schemaVersion is a string", with("/schemaVersion", json!("1")), false),
        case("schemaVersion 1.0 is the number 1", with("/schemaVersion", json!(1.0)), true),
        case("name of 120 characters", with("/name", json!("n".repeat(120))), true),
        case("name of 121 characters", with("/name", json!("n".repeat(121))), false),
        case("name of 120 multibyte characters", with("/name", json!("é".repeat(120))), true),
        case("name of 120 four-byte characters", with("/name", json!("😀".repeat(120))), true),
        case("name of 121 four-byte characters", with("/name", json!("😀".repeat(121))), false),
        case("name is a number", with("/name", json!(7)), false),
        case("no name", minus("", "name"), true),
        case("standardAnimationSet 1", plus("", "standardAnimationSet", json!(1)), true),
        case("standardAnimationSet 2", plus("", "standardAnimationSet", json!(2)), false),
        case("standardAnimationSet true", plus("", "standardAnimationSet", json!(true)), false),
        case("an unknown member", plus("", "extra", json!(1)), false),
    ];
    for k in ["schema", "schemaVersion", "body", "colors", "expressions", "expressionOrder", "animations", "animationOrder"] {
        cases.push(case(&format!("missing {k}"), minus("", k), false));
    }
    expect(cases);
}

#[test]
fn parse_rule_2_every_object_rejects_members_it_does_not_list() {
    let n = json!(1);
    let paths = [
        "/body",
        "/body/primary",
        "/colors",
        "/expressions/calm",
        "/expressions/calm/head",
        "/expressions/calm/eyes",
        "/expressions/calm/eyes/left",
        "/expressions/calm/eyes/right",
        "/expressions/calm/motion",
        "/animations/idle",
        "/animations/idle/steps/0",
        "/animations/idle/blink",
    ];
    let mut cases: Vec<_> = paths.iter().map(|p| case(&format!("extra member in {p}"), plus(p, "extra", n.clone()), false)).collect();
    let mut node = base();
    node["body"]["nodes"] = json!([{ "surface": { "type": "cube", "width": 1, "height": 1, "depth": 1, "roundness": 0 }, "position": [0, 0, 0], "rotation": [0, 0, 0] }]);
    for p in ["/body/nodes/0", "/body/nodes/0/surface"] {
        let mut v = node.clone();
        v.pointer_mut(p).unwrap().as_object_mut().unwrap().insert("extra".into(), n.clone());
        cases.push(case(&format!("extra member in {p}"), v, false));
    }
    cases.push(case("the node base is itself valid", node, true));
    let mut meta = base();
    meta["animations"]["idle"]["metadata"] = json!({ "label": "x", "extra": 1 });
    cases.push(case("extra member in metadata", meta, false));
    let mut ec = base();
    ec["expressions"]["calm"]["colors"] = json!({ "body": "#112233", "extra": 1 });
    cases.push(case("extra member in an expression's colors", ec, false));
    expect(cases);
}

#[test]
fn parse_rule_2_reports_an_unlisted_member_by_its_pointer() {
    let r = reason_of(&text(&plus("", "bogus", json!(1))));
    assert!(r.starts_with("/bogus: "), "reason was {r:?}");
    let r = reason_of(&text(&plus("/colors", "tint", json!(1))));
    assert!(r.starts_with("/colors/tint: "), "reason was {r:?}");
}

#[test]
fn parse_rule_2_escapes_the_pointer_per_rfc_6901() {
    let r = reason_of(&text(&plus("", "a/b", json!(1))));
    assert!(r.starts_with("/a~1b: "), "reason was {r:?}");
    let r = reason_of(&text(&plus("", "a~b", json!(1))));
    assert!(r.starts_with("/a~0b: "), "reason was {r:?}");
}

#[test]
fn parse_rule_2_keys_match_the_semantic_pattern() {
    let rename = |new: &str| {
        let mut v = base();
        let e = v["expressions"].as_object_mut().unwrap().remove("calm").unwrap();
        v["expressions"].as_object_mut().unwrap().insert(new.to_string(), e);
        v["expressionOrder"] = json!(["neutral", new]);
        v["animations"]["idle"]["steps"][0]["expression"] = json!(new);
        v
    };
    let long_ok = key_of(64);
    let long_bad = key_of(65);
    let mut cases = vec![];
    for (name, key, ok) in [
        ("lowercase letters", "calm", true),
        ("digits after the first letter", "a1", true),
        ("hyphenated", "calm-down", true),
        ("hyphen then digits", "a-1", true),
        ("64 characters", long_ok.as_str(), true),
        ("65 characters", long_bad.as_str(), false),
        ("uppercase", "Calm", false),
        ("leading digit", "1calm", false),
        ("leading hyphen", "-calm", false),
        ("trailing hyphen", "calm-", false),
        ("double hyphen", "calm--down", false),
        ("underscore", "calm_down", false),
        ("space", "calm down", false),
        ("empty", "", false),
        ("non ASCII letter", "calmé", false),
    ] {
        cases.push(case(&format!("expression key, {name}"), rename(key), ok));
    }
    // The same pattern holds for animation names.
    let mut anim = base();
    let a = anim["animations"].as_object_mut().unwrap().remove("idle").unwrap();
    anim["animations"].as_object_mut().unwrap().insert("Idle".into(), a);
    anim["animationOrder"] = json!(["Idle"]);
    cases.push(case("animation key with uppercase", anim, false));
    for (len, ok) in [(64, true), (65, false)] {
        let mut anim = base();
        let a = anim["animations"].as_object_mut().unwrap().remove("idle").unwrap();
        anim["animations"].as_object_mut().unwrap().insert(key_of(len), a);
        anim["animationOrder"] = json!([key_of(len)]);
        cases.push(case(&format!("animation key of {len} characters"), anim, ok));
    }
    expect(cases);
}

#[test]
fn parse_rule_2_colors_are_lowercase_six_digit_hex() {
    let mut rows: Vec<(&str, Value, bool)> = vec![];
    for (c, ok) in [
        ("#123abc", true),
        ("#000000", true),
        ("#ffffff", true),
        ("#ABCDEF", false),
        ("#123ABC", false),
        ("#abc", false),
        ("#12345", false),
        ("#1234567", false),
        ("123abc", false),
        ("#12345g", false),
        ("#12345678", false),
        ("red", false),
        ("", false),
    ] {
        rows.push(("/colors/body", json!(c), ok));
        rows.push(("/colors/eyes", json!(c), ok));
    }
    rows.push(("/colors/body", json!(1), false));
    table(&rows);
    let mut v = base();
    v["expressions"]["calm"]["colors"] = json!({ "body": "#112233" });
    assert_eq!(parse(&text(&v)), Ok(v.clone()), "an expression may override only the body color");
    v["expressions"]["calm"]["colors"] = json!({ "eyes": "#112233" });
    assert_eq!(parse(&text(&v)), Ok(v.clone()), "or only the eye color");
    v["expressions"]["calm"]["colors"] = json!({ "body": "#112233", "eyes": "#445566" });
    assert_eq!(parse(&text(&v)), Ok(v.clone()));
    v["expressions"]["calm"]["colors"] = json!({});
    assert!(parse(&text(&v)).is_err(), "an expression's colors needs at least one of body, eyes");
    v["expressions"]["calm"]["colors"] = json!({ "body": "#ABCDEF" });
    assert!(parse(&text(&v)).is_err());
}

#[test]
fn parse_rule_2_expressions_and_orders_have_their_size_limits() {
    let many = |n: usize| {
        let mut v = base();
        let mut order = vec![json!("neutral")];
        for i in 0..n - 1 {
            let k = format!("e{i}");
            v["expressions"].as_object_mut().unwrap().insert(k.clone(), expr(i as f64));
            order.push(json!(k));
        }
        v["expressions"].as_object_mut().unwrap().remove("calm");
        v["animations"]["idle"]["steps"][0]["expression"] = json!("e0");
        v["expressionOrder"] = Value::Array(order);
        v
    };
    let anims = |n: usize| {
        let mut v = base();
        let mut order = vec![];
        v["animations"] = json!({});
        for i in 0..n {
            let k = format!("a{i}");
            v["animations"].as_object_mut().unwrap().insert(k.clone(), animation());
            order.push(json!(k));
        }
        v["animationOrder"] = Value::Array(order);
        v
    };
    let steps = |n: usize| with("/animations/idle/steps", Value::Array(vec![animation()["steps"][0].clone(); n]));
    let mut no_neutral = base();
    no_neutral["expressions"].as_object_mut().unwrap().remove("neutral");
    no_neutral["expressionOrder"] = json!(["calm"]);
    let mut only_neutral = base();
    only_neutral["expressions"].as_object_mut().unwrap().remove("calm");
    only_neutral["expressionOrder"] = json!(["neutral"]);
    only_neutral["animations"]["idle"]["steps"][0]["expression"] = json!("neutral");
    let mut no_animations = base();
    no_animations["animations"] = json!({});
    no_animations["animationOrder"] = json!([]);
    let mut dup_expr_order = base();
    dup_expr_order["expressionOrder"] = json!(["neutral", "calm", "calm"]);
    let mut dup_anim_order = base();
    dup_anim_order["animationOrder"] = json!(["idle", "idle"]);
    expect(vec![
        case("128 expressions", many(128), true),
        case("129 expressions", many(129), false),
        case("neutral is required", no_neutral, false),
        case("only neutral", only_neutral, true),
        case("64 animations", anims(64), true),
        case("65 animations", anims(65), false),
        case("no animations", no_animations, true),
        case("128 steps", steps(128), true),
        case("129 steps", steps(129), false),
        case("0 steps", with("/animations/idle/steps", json!([])), false),
        case("a repeated expressionOrder entry", dup_expr_order, false),
        case("a repeated animationOrder entry", dup_anim_order, false),
        case("empty expressionOrder", with("/expressionOrder", json!([])), false),
    ]);
}

#[test]
fn parse_rule_2_body_surface_and_nodes() {
    let node = |ty: &str| json!({ "surface": { "type": ty, "width": 1, "height": 1, "depth": 1, "roundness": 0 }, "position": [0, 0, 0], "rotation": [0, 0, 0] });
    let nodes = |n: usize| with("/body/nodes", Value::Array(vec![node("cube"); n]));
    let mut cases = vec![
        case("16 body nodes", nodes(16), true),
        case("17 body nodes", nodes(17), false),
        case("no nodes member", minus("/body", "nodes"), false),
        case("no primary member", minus("/body", "primary"), false),
    ];
    for t in ["sphere", "mickey", "cursor", "cube", "capsule", "cylinder", "cone", "diamond"] {
        cases.push(case(&format!("primary surface {t}"), with("/body/primary/type", json!(t)), true));
    }
    for t in ["sphere", "cube", "capsule", "cylinder", "cone", "diamond"] {
        cases.push(case(&format!("node surface {t}"), with("/body/nodes", json!([node(t)])), true));
    }
    for t in ["mickey", "cursor"] {
        cases.push(case(&format!("node surface {t}"), with("/body/nodes", json!([node(t)])), false));
    }
    for t in ["torus", "Sphere", ""] {
        cases.push(case(&format!("primary surface {t:?}"), with("/body/primary/type", json!(t)), false));
    }
    for m in ["morphRoundness", "tipRoundness", "baseRoundness"] {
        cases.push(case(&format!("optional {m} 0.5"), plus("/body/primary", m, json!(0.5)), true));
        cases.push(case(&format!("optional {m} 1.01"), plus("/body/primary", m, json!(1.01)), false));
    }
    for m in ["width", "height", "depth", "roundness", "type"] {
        cases.push(case(&format!("surface without {m}"), minus("/body/primary", m), false));
    }
    expect(cases);
}

#[test]
fn parse_rule_2_numeric_bounds_at_and_just_past_each_limit() {
    let mut rows: Vec<(&str, Value, bool)> = vec![];
    for p in ["/body/primary/width", "/body/primary/height", "/body/primary/depth"] {
        rows.extend([
            (p, json!(0.001), true),
            (p, json!(0.0009), false),
            (p, json!(0), false),
            (p, json!(-1), false),
            (p, json!(10000), true),
            (p, json!(10000.001), false),
        ]);
    }
    rows.extend([
        ("/body/primary/roundness", json!(0), true),
        ("/body/primary/roundness", json!(1), true),
        ("/body/primary/roundness", json!(-0.001), false),
        ("/body/primary/roundness", json!(1.001), false),
        ("/expressions/calm/perspective", json!(0.1), true),
        ("/expressions/calm/perspective", json!(0.0999), false),
        ("/expressions/calm/perspective", json!(10), true),
        ("/expressions/calm/perspective", json!(10.001), false),
        ("/animations/idle/steps/0/holdMs", json!(100), true),
        ("/animations/idle/steps/0/holdMs", json!(99.9), false),
        ("/animations/idle/steps/0/holdMs", json!(60000), true),
        ("/animations/idle/steps/0/holdMs", json!(60000.1), false),
        ("/animations/idle/steps/0/transitionMs", json!(0), true),
        ("/animations/idle/steps/0/transitionMs", json!(-0.1), false),
        ("/animations/idle/steps/0/transitionMs", json!(5000), true),
        ("/animations/idle/steps/0/transitionMs", json!(5000.1), false),
        ("/animations/idle/blink/initialDelayMs", json!(0), true),
        ("/animations/idle/blink/initialDelayMs", json!(-1), false),
        ("/animations/idle/blink/initialDelayMs", json!(60000), true),
        ("/animations/idle/blink/initialDelayMs", json!(60001), false),
        ("/animations/idle/blink/minIntervalMs", json!(250), true),
        ("/animations/idle/blink/minIntervalMs", json!(249), false),
        ("/animations/idle/blink/maxIntervalMs", json!(250), true),
        ("/animations/idle/blink/maxIntervalMs", json!(249), false),
        ("/animations/idle/blink/maxIntervalMs", json!(120000), true),
        ("/animations/idle/blink/maxIntervalMs", json!(120001), false),
        ("/animations/idle/blink/durationMs", json!(50), true),
        ("/animations/idle/blink/durationMs", json!(49), false),
        ("/animations/idle/blink/durationMs", json!(2000), true),
        ("/animations/idle/blink/durationMs", json!(2001), false),
        ("/expressions/calm/head/x", json!(10000), true),
        ("/expressions/calm/head/x", json!(10000.5), false),
        ("/expressions/calm/head/y", json!(-10000), true),
        ("/expressions/calm/head/y", json!(-10000.5), false),
        ("/expressions/calm/head/z", json!(10001), false),
        ("/expressions/calm/eyes/spacing", json!(-10000), true),
        ("/expressions/calm/eyes/spacing", json!(-10001), false),
        ("/expressions/calm/eyes/left/width", json!(-10000), true),
        ("/expressions/calm/eyes/left/width", json!(10001), false),
        ("/expressions/calm/eyes/left/height", json!(10000), true),
        ("/expressions/calm/eyes/left/height", json!(-10001), false),
        ("/expressions/calm/eyes/left/x", json!(10001), false),
        ("/expressions/calm/eyes/left/y", json!(-10001), false),
        ("/expressions/calm/eyes/right/angle", json!(10000), true),
        ("/expressions/calm/eyes/right/angle", json!(10001), false),
        ("/expressions/calm/eyes/right/x", json!(10001), false),
    ]);
    table(&rows);
}

#[test]
fn parse_rule_2_node_position_and_rotation_bounds() {
    let with_node = |pos: Value, rot: Value| {
        with(
            "/body/nodes",
            json!([{ "surface": { "type": "cube", "width": 1, "height": 1, "depth": 1, "roundness": 0 }, "position": pos, "rotation": rot }]),
        )
    };
    expect(vec![
        case("position at 10000", with_node(json!([10000, -10000, 0]), json!([0, 0, 0])), true),
        case("position x past 10000", with_node(json!([10000.1, 0, 0]), json!([0, 0, 0])), false),
        case("position y past -10000", with_node(json!([0, -10000.1, 0]), json!([0, 0, 0])), false),
        case("position z past 10000", with_node(json!([0, 0, 10001]), json!([0, 0, 0])), false),
        case("position with 2 numbers", with_node(json!([0, 0]), json!([0, 0, 0])), false),
        case("position with 4 numbers", with_node(json!([0, 0, 0, 0]), json!([0, 0, 0])), false),
        case("position with a string", with_node(json!([0, "0", 0]), json!([0, 0, 0])), false),
        case("rotation at 360 and -360", with_node(json!([0, 0, 0]), json!([360, -360, 360])), true),
        case("rotation x past 360", with_node(json!([0, 0, 0]), json!([360.1, 0, 0])), false),
        case("rotation y past -360", with_node(json!([0, 0, 0]), json!([0, -360.1, 0])), false),
        case("rotation z past 360", with_node(json!([0, 0, 0]), json!([0, 0, 361])), false),
        case("rotation with 2 numbers", with_node(json!([0, 0, 0]), json!([0, 0])), false),
        case("rotation with 4 numbers", with_node(json!([0, 0, 0]), json!([0, 0, 0, 0])), false),
    ]);
}

#[test]
fn parse_rule_2_enums() {
    let mut rows: Vec<(&str, Value, bool)> = vec![];
    for (p, good, bad) in [
        ("/expressions/calm/motion/eyes", ["none", "microSaccades", "shake"], "slowDrift"),
        ("/expressions/calm/motion/body", ["none", "slowDrift", "shake"], "microSaccades"),
        ("/animations/idle/playbackMode", ["loop", "once", "pingPong"], "bounce"),
        ("/animations/idle/steps/0/transition", ["spring", "smooth", "snappy"], "linear"),
    ] {
        for g in good {
            rows.push((p, json!(g), true));
        }
        rows.push((p, json!(bad), false));
        rows.push((p, json!(good[0].to_uppercase()), false));
        rows.push((p, json!(1), false));
    }
    table(&rows);
}

#[test]
fn parse_rule_2_types_are_strict() {
    table(&[
        ("/body/primary/width", json!("100"), false),
        ("/body/primary/width", json!(null), false),
        ("/body/primary/width", json!(true), false),
        ("/expressions/calm/head/x", json!("0"), false),
        ("/animations/idle/blink/enabled", json!(0), false),
        ("/animations/idle/blink/enabled", json!("false"), false),
        ("/animations/idle/blink/enabled", json!(true), true),
        ("/animations/idle/steps", json!({}), false),
        ("/expressionOrder", json!("neutral"), false),
        ("/body/nodes", json!({}), false),
        ("/colors", json!([]), false),
        ("/expressions", json!([]), false),
    ]);
}

#[test]
fn parse_rule_2_animation_metadata() {
    let meta = |m: Value| {
        let mut v = base();
        v["animations"]["idle"]["metadata"] = m;
        v
    };
    expect(vec![
        case("label, description and group at their limits", meta(json!({ "label": "l".repeat(120), "description": "d".repeat(512), "group": "g".repeat(64) })), true),
        case("only a label", meta(json!({ "label": "x" })), true),
        case("only a description", meta(json!({ "description": "x" })), true),
        case("only a group", meta(json!({ "group": "x" })), true),
        case("empty metadata", meta(json!({})), false),
        case("label of 121", meta(json!({ "label": "l".repeat(121) })), false),
        case("description of 513", meta(json!({ "description": "d".repeat(513) })), false),
        case("group of 65", meta(json!({ "group": "g".repeat(65) })), false),
        case("label of 120 four-byte characters", meta(json!({ "label": "😀".repeat(120) })), true),
        case("label of 121 four-byte characters", meta(json!({ "label": "😀".repeat(121) })), false),
        case("label is a number", meta(json!({ "label": 1 })), false),
    ]);
}

#[test]
fn parse_rule_2_required_members_of_nested_objects() {
    let mut cases = vec![];
    for (parent, keys) in [
        ("/colors", vec!["body", "eyes"]),
        ("/expressions/calm", vec!["head", "eyes", "perspective", "motion"]),
        ("/expressions/calm/head", vec!["x", "y", "z"]),
        ("/expressions/calm/eyes", vec!["left", "right", "spacing"]),
        ("/expressions/calm/eyes/left", vec!["width", "height", "x", "y", "angle"]),
        ("/expressions/calm/motion", vec!["eyes", "body"]),
        ("/animations/idle", vec!["playbackMode", "steps", "blink"]),
        ("/animations/idle/blink", vec!["enabled", "initialDelayMs", "minIntervalMs", "maxIntervalMs", "durationMs"]),
        ("/animations/idle/steps/0", vec!["expression", "holdMs", "transitionMs", "transition"]),
    ] {
        for k in keys {
            cases.push(case(&format!("{parent} without {k}"), minus(parent, k), false));
        }
    }
    expect(cases);
}

#[test]
fn parse_rule_2_reports_an_out_of_range_member_by_its_pointer() {
    for (path, value) in [
        ("/animations/idle/steps/0/holdMs", json!(99)),
        ("/colors/body", json!("#FFFFFF")),
        ("/body/primary/width", json!(0)),
        ("/animations/idle/blink/durationMs", json!(2001)),
    ] {
        let r = reason_of(&text(&with(path, value)));
        assert!(r.starts_with(&format!("{path}: ")), "{path}: reason was {r:?}");
    }
}

// --- parse: rule 3, cross references -----------------------------------------------------------------------------

#[test]
fn parse_rule_3_expression_order() {
    let mut swapped = base();
    swapped["expressionOrder"] = json!(["calm", "neutral"]);
    let mut missing = base();
    missing["expressionOrder"] = json!(["neutral"]);
    let mut unknown = base();
    unknown["expressionOrder"] = json!(["neutral", "calm", "ghost"]);
    let mut renamed = base();
    renamed["expressionOrder"] = json!(["neutral", "ghost"]);
    expect(vec![
        case("neutral first, every key listed", base(), true),
        case("neutral not first", swapped.clone(), false),
        case("a key of expressions is missing from the order", missing, false),
        case("the order names a key that is not in expressions", unknown, false),
        case("the order swaps a key for an unknown one", renamed, false),
    ]);
    let r = reason_of(&text(&swapped));
    assert!(r.starts_with("/expressionOrder/0: "), "reason was {r:?}");
}

#[test]
fn parse_rule_3_animation_order() {
    let mut empty = base();
    empty["animationOrder"] = json!([]);
    let mut unknown = base();
    unknown["animationOrder"] = json!(["idle", "ghost"]);
    let mut other = base();
    other["animationOrder"] = json!(["ghost"]);
    let mut two = base();
    two["animations"]["wake"] = animation();
    let mut reordered = two.clone();
    reordered["animationOrder"] = json!(["wake", "idle"]);
    expect(vec![
        case("an animation missing from the order", empty, false),
        case("the order names an unknown animation", unknown, false),
        case("the order names only an unknown animation", other, false),
        case("a second animation not in the order", two, false),
        case("the order lists them in another order", reordered, true),
    ]);
}

#[test]
fn parse_rule_3_blink_interval_order() {
    let mut below = base();
    below["animations"]["idle"]["blink"]["maxIntervalMs"] = json!(251);
    let mut above = base();
    above["animations"]["idle"]["blink"]["minIntervalMs"] = json!(300);
    let mut disabled = base();
    disabled["animations"]["idle"]["blink"]["minIntervalMs"] = json!(1000);
    disabled["animations"]["idle"]["blink"]["maxIntervalMs"] = json!(500);
    expect(vec![
        case("min equal to max", base(), true),
        case("min below max", below, true),
        case("min above max", above.clone(), false),
        case("min above max while blink is disabled", disabled, false),
    ]);
    let r = reason_of(&text(&above));
    assert!(r.starts_with("/animations/idle/blink/minIntervalMs: "), "reason was {r:?}");
}

#[test]
fn parse_rule_3_step_expression_must_exist() {
    let r = reason_of(&text(&with("/animations/idle/steps/0/expression", json!("nap"))));
    assert!(r.starts_with("/animations/idle/steps/0/expression: "), "reason was {r:?}");
    let mut second = base();
    second["animations"]["idle"]["steps"] = json!([
        { "expression": "calm", "holdMs": 100, "transitionMs": 0, "transition": "snappy" },
        { "expression": "nap", "holdMs": 100, "transitionMs": 0, "transition": "snappy" }
    ]);
    let r = reason_of(&text(&second));
    assert!(r.starts_with("/animations/idle/steps/1/expression: "), "reason was {r:?}");
    assert!(parse(&text(&with("/animations/idle/steps/0/expression", json!("neutral")))).is_ok());
}

// --- load ---------------------------------------------------------------------------------------------------------

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("notch-avatar-tests-{}-{}", std::process::id(), name));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn load_a_missing_file_is_no_file() {
    let d = tmp("missing");
    assert_eq!(load(&d.join("nope.json")), Err("no file".to_string()));
    assert_eq!(load(&d.join("no-dir").join("nope.json")), Err("no file".to_string()));
}

#[test]
fn load_a_valid_file_is_its_parsed_json() {
    let d = tmp("valid");
    let p = d.join("a.json");
    fs::write(&p, text(&base())).unwrap();
    assert_eq!(load(&p), Ok(base()));
}

#[test]
fn load_an_invalid_file_gives_the_parse_reason() {
    let d = tmp("invalid");
    let p = d.join("a.json");
    let bad = text(&with("/animations/idle/steps/0/expression", json!("nap")));
    fs::write(&p, &bad).unwrap();
    assert_eq!(load(&p), parse(&bad));
    assert!(load(&p).unwrap_err().starts_with("/animations/idle/steps/0/expression: "));
}

#[test]
fn load_a_file_over_max_bytes_is_rejected_with_the_exact_reason() {
    let d = tmp("big");
    let t = text(&base());
    let p = d.join("big.json");
    fs::write(&p, format!("{t}{}", " ".repeat(MAX_BYTES + 1 - t.len()))).unwrap();
    assert_eq!(fs::metadata(&p).unwrap().len() as usize, MAX_BYTES + 1);
    assert_eq!(load(&p), Err("larger than 262144 bytes".to_string()));
    let huge = d.join("huge.json");
    fs::write(&huge, vec![b' '; MAX_BYTES * 8]).unwrap();
    assert_eq!(load(&huge), Err("larger than 262144 bytes".to_string()));
}

#[test]
fn load_a_file_of_exactly_max_bytes_is_read() {
    let d = tmp("exact");
    let t = text(&base());
    let p = d.join("exact.json");
    fs::write(&p, format!("{t}{}", " ".repeat(MAX_BYTES - t.len()))).unwrap();
    assert_eq!(load(&p), Ok(base()));
}

#[test]
fn load_bytes_that_are_not_utf8_are_an_error_that_is_not_no_file() {
    let d = tmp("utf8");
    let p = d.join("bad.json");
    let mut bytes = text(&base()).into_bytes();
    bytes.splice(10..10, [0xff, 0xfe, 0xfd]);
    fs::write(&p, &bytes).unwrap();
    let e = load(&p).expect_err("not UTF-8");
    assert_ne!(e, "no file");
    assert!(!e.is_empty());
    fs::write(&p, [0xc3u8, 0x28]).unwrap();
    assert!(load(&p).is_err());
}

#[test]
fn load_an_empty_file_is_an_error() {
    let d = tmp("odd");
    let p = d.join("empty.json");
    fs::write(&p, "").unwrap();
    assert!(load(&p).is_err());
    assert_ne!(load(&p), Err("no file".to_string()));
    assert_ne!(load(&p), Err("not a file".to_string()));
}

#[test]
fn load_a_directory_is_not_a_file() {
    let d = tmp("dir");
    let sub = d.join("dir.json");
    fs::create_dir_all(&sub).unwrap();
    assert_eq!(load(&sub), Err("not a file".to_string()));
    assert_eq!(load(&d), Err("not a file".to_string()));
}

#[test]
fn load_a_fifo_is_not_a_file_and_returns_promptly() {
    let d = tmp("fifo");
    let p = d.join("pipe.json");
    let made = std::process::Command::new("mkfifo").arg(&p).status().expect("mkfifo");
    assert!(made.success());
    let (tx, rx) = std::sync::mpsc::channel();
    let q = p.clone();
    std::thread::spawn(move || {
        let _ = tx.send(load(&q));
    });
    let got = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("load blocked on a FIFO");
    assert_eq!(got, Err("not a file".to_string()));
}

#[test]
fn load_follows_a_symlink_to_a_valid_file() {
    let d = tmp("symlink");
    let target = d.join("real.json");
    fs::write(&target, text(&base())).unwrap();
    let link = d.join("link.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(load(&link), Ok(base()));
}

// --- avatar_path --------------------------------------------------------------------------------------------------

fn os(s: &str) -> std::ffi::OsString {
    std::ffi::OsString::from(s)
}

#[test]
fn avatar_path_from_is_notch_avatar_when_present() {
    let p = avatar_path_from(Some(os("/tmp/somewhere/mine.avatar.json").as_os_str()), Some(os("/Users/someone").as_os_str()));
    assert_eq!(p, PathBuf::from("/tmp/somewhere/mine.avatar.json"));
}

#[test]
fn avatar_path_from_ignores_an_empty_notch_avatar() {
    let p = avatar_path_from(Some(os("").as_os_str()), Some(os("/Users/someone").as_os_str()));
    assert_eq!(p, PathBuf::from("/Users/someone/Library/Application Support/notch-hud/avatar.json"));
}

#[test]
fn avatar_path_from_an_absent_notch_avatar_is_under_home() {
    let p = avatar_path_from(None, Some(os("/Users/someone").as_os_str()));
    assert_eq!(p, PathBuf::from("/Users/someone/Library/Application Support/notch-hud/avatar.json"));
}

#[test]
fn avatar_path_from_notch_avatar_wins_without_a_home() {
    let p = avatar_path_from(Some(os("/tmp/mine.json").as_os_str()), None);
    assert_eq!(p, PathBuf::from("/tmp/mine.json"));
}

#[test]
fn avatar_path_from_without_a_home_is_the_relative_path() {
    let rel = PathBuf::from("Library/Application Support/notch-hud/avatar.json");
    assert_eq!(avatar_path_from(None, None), rel);
    assert_eq!(avatar_path_from(Some(os("").as_os_str()), None), rel);
}

#[test]
fn avatar_path_from_keeps_a_non_utf8_notch_avatar() {
    use std::os::unix::ffi::OsStrExt;
    let raw = std::ffi::OsStr::from_bytes(b"/tmp/caf\xe9.json");
    assert_eq!(avatar_path_from(Some(raw), Some(os("/Users/someone").as_os_str())), PathBuf::from(raw));
}

#[test]
fn avatar_path_is_avatar_path_from_the_environment() {
    // NOTCH_AVATAR is changed by no test, so it can be compared exactly. HOME is changed for a moment by a session
    // test, so with NOTCH_AVATAR unset only the part that does not depend on HOME is asserted.
    let p = avatar_path();
    assert_ne!(p, PathBuf::new());
    match std::env::var_os("NOTCH_AVATAR") {
        Some(v) if !v.is_empty() => assert_eq!(p, PathBuf::from(v)),
        _ => assert!(p.ends_with("Library/Application Support/notch-hud/avatar.json"), "got {p:?}"),
    }
}

// --- DEFAULT and the pose table ----------------------------------------------------------------------------------

const MOODS: [Mood; 8] = [
    Mood::Alert,
    Mood::Error,
    Mood::Celebrate,
    Mood::Working,
    Mood::Listening,
    Mood::Idle,
    Mood::Drowsy,
    Mood::Sleeping,
];

#[test]
fn the_default_avatar_parses() {
    assert!(parse(DEFAULT).is_ok(), "{:?}", parse(DEFAULT));
}

#[test]
fn the_default_avatar_is_under_the_size_limit() {
    assert!(DEFAULT.len() <= MAX_BYTES);
}

#[test]
fn the_default_avatar_defines_the_first_animation_of_every_mood_row() {
    let def = parse(DEFAULT).unwrap();
    let firsts = ["surprised", "sad", "celebrate", "working", "listening", "idle", "drowsy", "sleeping"];
    for (m, first) in MOODS.iter().zip(firsts) {
        assert!(def["animations"].get(first).map_or(false, |a| a.is_object()), "no animation {first} for {m:?}");
    }
}

#[test]
fn pose_is_the_contract_table() {
    let table: [(Mood, &'static [&'static str], bool); 8] = [
        (Mood::Alert, &["surprised", "curious"], false),
        (Mood::Error, &["sad", "angry"], false),
        (Mood::Celebrate, &["celebrate", "happy", "excited"], false),
        (Mood::Working, &["working", "thinking"], false),
        (Mood::Listening, &["listening"], false),
        (Mood::Idle, &["idle"], false),
        (Mood::Drowsy, &["drowsy", "bored"], false),
        (Mood::Sleeping, &["sleeping"], true),
    ];
    for (mood, animations, still) in table {
        assert_eq!(pose(mood), Pose { mood, animations, still }, "{mood:?}");
    }
}

#[test]
fn mood_and_pose_serialize_as_snake_case_json() {
    let names = ["alert", "error", "celebrate", "working", "listening", "idle", "drowsy", "sleeping"];
    for (m, n) in MOODS.iter().zip(names) {
        assert_eq!(serde_json::to_value(m).unwrap(), json!(n));
    }
    assert_eq!(
        serde_json::to_value(pose(Mood::Celebrate)).unwrap(),
        json!({ "mood": "celebrate", "animations": ["celebrate", "happy", "excited"], "still": false })
    );
    assert_eq!(
        serde_json::to_value(pose(Mood::Sleeping)).unwrap(),
        json!({ "mood": "sleeping", "animations": ["sleeping"], "still": true })
    );
}

#[test]
fn source_and_avatar_serialize_as_the_avatar_command_reply() {
    assert_eq!(serde_json::to_value(Source::User).unwrap(), json!("user"));
    assert_eq!(serde_json::to_value(Source::Default).unwrap(), json!("default"));
    let a = Avatar { source: Source::Default, definition: json!({ "k": 1 }) };
    assert_eq!(serde_json::to_value(a).unwrap(), json!({ "source": "default", "definition": { "k": 1 } }));
}

// --- signals and finished ----------------------------------------------------------------------------------------

fn row(label: &str, status: Status) -> Row {
    Row { id: label.to_string(), label: label.to_string(), status, detail: None }
}

fn summary(rows: Vec<Row>, working: usize) -> Summary {
    let attention = rows.iter().filter(|r| matches!(r.status, Status::NeedsPermission | Status::NeedsInput | Status::Error)).count();
    Summary { total: rows.len(), attention, working, rows }
}

fn track(playing: bool) -> Track {
    Track {
        title: "Low Tide Static".into(),
        artist: Some("Mara Venn".into()),
        album: None,
        app: "Apple Music".into(),
        bundle_id: "com.apple.Music".into(),
        playing,
        duration_ms: Some(223_000),
        elapsed_ms: Some(74_000),
        elapsed_at_ms: Some(T),
        artwork: None,
    }
}

fn media(t: Option<Track>) -> Media {
    Media { via: Via::Adapter, health: Health::Ok, track: t }
}

fn sig(needs_you: bool, error: bool, working: bool, playing: bool) -> Signals {
    Signals { needs_you, error, working, playing }
}

#[test]
fn signals_default_is_all_false() {
    assert_eq!(Signals::default(), sig(false, false, false, false));
    assert_eq!(signals(None, None), Signals::default());
}

#[test]
fn signals_needs_you_from_a_permission_or_input_row() {
    let s = summary(vec![row("a", Status::NeedsPermission)], 0);
    assert_eq!(signals(Some(&s), None), sig(true, false, false, false));
    let s = summary(vec![row("a", Status::NeedsInput)], 0);
    assert_eq!(signals(Some(&s), None), sig(true, false, false, false));
}

#[test]
fn signals_error_from_an_error_row() {
    let s = summary(vec![row("a", Status::Error)], 0);
    assert_eq!(signals(Some(&s), None), sig(false, true, false, false));
}

#[test]
fn signals_done_and_waiting_rows_set_nothing() {
    let s = summary(vec![row("a", Status::Done), row("b", Status::Waiting)], 0);
    assert_eq!(signals(Some(&s), None), Signals::default());
}

#[test]
fn signals_a_row_further_down_still_counts() {
    let s = summary(vec![row("a", Status::Done), row("b", Status::Waiting), row("c", Status::Error), row("d", Status::NeedsInput)], 0);
    assert_eq!(signals(Some(&s), None), sig(true, true, false, false));
}

#[test]
fn signals_working_is_the_summary_count_not_the_rows() {
    let counted = summary(vec![], 1);
    assert_eq!(signals(Some(&counted), None), sig(false, false, true, false));
    let rows_only = Summary { rows: vec![row("a", Status::Working)], total: 1, attention: 0, working: 0 };
    assert_eq!(signals(Some(&rows_only), None), Signals::default());
    let many = summary(vec![row("a", Status::Working), row("b", Status::Working)], 2);
    assert_eq!(signals(Some(&many), None), sig(false, false, true, false));
}

#[test]
fn signals_playing_is_a_track_that_is_playing() {
    assert_eq!(signals(None, Some(&media(Some(track(true))))), sig(false, false, false, true));
    assert_eq!(signals(None, Some(&media(Some(track(false))))), Signals::default());
    assert_eq!(signals(None, Some(&media(None))), Signals::default());
}

#[test]
fn signals_combines_both_inputs() {
    let s = summary(vec![row("a", Status::NeedsPermission), row("b", Status::Working)], 1);
    assert_eq!(signals(Some(&s), Some(&media(Some(track(true))))), sig(true, false, true, true));
    let empty = summary(vec![], 0);
    assert_eq!(signals(Some(&empty), Some(&media(Some(track(true))))), sig(false, false, false, true));
}

#[test]
fn finished_is_false_without_a_previous_summary() {
    let next = summary(vec![row("a", Status::Done)], 0);
    assert!(!finished(None, &next));
}

#[test]
fn finished_when_a_row_became_done() {
    let prev = summary(vec![row("a", Status::Working)], 1);
    let next = summary(vec![row("a", Status::Done)], 0);
    assert!(finished(Some(&prev), &next));
}

#[test]
fn finished_from_any_other_status_to_done() {
    for from in [Status::NeedsPermission, Status::NeedsInput, Status::Error, Status::Working, Status::Waiting] {
        let prev = summary(vec![row("a", from)], 0);
        let next = summary(vec![row("a", Status::Done)], 0);
        assert!(finished(Some(&prev), &next), "{from:?} to done");
    }
}

#[test]
fn finished_is_false_when_the_row_was_already_done() {
    let prev = summary(vec![row("a", Status::Done)], 0);
    let next = summary(vec![row("a", Status::Done)], 0);
    assert!(!finished(Some(&prev), &next));
}

#[test]
fn finished_is_false_when_the_label_is_new() {
    let prev = summary(vec![row("a", Status::Working)], 1);
    let next = summary(vec![row("a", Status::Working), row("b", Status::Done)], 1);
    assert!(!finished(Some(&prev), &next));
    let empty = summary(vec![], 0);
    assert!(!finished(Some(&empty), &next));
}

#[test]
fn finished_is_false_when_the_row_is_not_done_now() {
    for to in [Status::NeedsPermission, Status::NeedsInput, Status::Error, Status::Working, Status::Waiting] {
        let prev = summary(vec![row("a", Status::Done)], 0);
        let next = summary(vec![row("a", to)], 0);
        assert!(!finished(Some(&prev), &next), "done to {to:?}");
    }
}

#[test]
fn finished_looks_at_every_row_of_next() {
    let prev = summary(vec![row("a", Status::Done), row("b", Status::Working), row("c", Status::Working)], 2);
    let next = summary(vec![row("a", Status::Done), row("b", Status::Working), row("c", Status::Done)], 1);
    assert!(finished(Some(&prev), &next));
}

#[test]
fn finished_matches_rows_by_label_not_by_position() {
    let prev = summary(vec![row("a", Status::Working), row("b", Status::Done)], 1);
    let next = summary(vec![row("b", Status::Done), row("a", Status::Working)], 1);
    assert!(!finished(Some(&prev), &next));
    let next = summary(vec![row("b", Status::Done), row("a", Status::Done)], 0);
    assert!(finished(Some(&prev), &next));
}

// --- Moods --------------------------------------------------------------------------------------------------------

const QUIET: Signals = Signals { needs_you: false, error: false, working: false, playing: false };

fn moods() -> Moods {
    Moods::new(T)
}

#[test]
fn the_time_constants_are_the_contract_values() {
    assert_eq!(CELEBRATE_MS, 4_000);
    assert_eq!(DROWSY_MS, 120_000);
    assert_eq!(SLEEP_MS, 300_000);
}

#[test]
fn each_condition_alone_gives_its_row() {
    let cases = [
        (sig(true, false, false, false), Mood::Alert),
        (sig(false, true, false, false), Mood::Error),
        (sig(false, false, true, false), Mood::Working),
        (sig(false, false, false, true), Mood::Listening),
        (QUIET, Mood::Idle),
    ];
    for (s, want) in cases {
        let mut m = moods();
        assert_eq!(m.step(s, false, T + 10), want, "{s:?}");
    }
}

#[test]
fn finished_alone_is_celebrate() {
    let mut m = moods();
    assert_eq!(m.step(QUIET, true, T + 10), Mood::Celebrate);
}

#[test]
fn a_higher_row_wins_over_every_lower_one() {
    let mut m = moods();
    assert_eq!(m.step(sig(true, true, true, true), true, T + 10), Mood::Alert, "needs_you beats error, celebrate, working, listening");
    let mut m = moods();
    assert_eq!(m.step(sig(false, true, true, true), true, T + 10), Mood::Error, "error beats celebrate, working, listening");
    let mut m = moods();
    assert_eq!(m.step(sig(false, false, true, true), true, T + 10), Mood::Celebrate, "celebrate beats working and listening");
    let mut m = moods();
    assert_eq!(m.step(sig(false, false, true, true), false, T + 10), Mood::Working, "working beats listening");
    let mut m = moods();
    assert_eq!(m.step(sig(false, false, false, true), false, T + 10), Mood::Listening);
}

#[test]
fn pairs_of_conditions_follow_the_table_order() {
    let pairs = [
        (sig(true, true, false, false), Mood::Alert),
        (sig(true, false, true, false), Mood::Alert),
        (sig(true, false, false, true), Mood::Alert),
        (sig(false, true, true, false), Mood::Error),
        (sig(false, true, false, true), Mood::Error),
        (sig(false, false, true, true), Mood::Working),
    ];
    for (s, want) in pairs {
        let mut m = moods();
        assert_eq!(m.step(s, false, T + 10), want, "{s:?}");
    }
}

#[test]
fn celebrate_beats_listening_but_not_alert_or_error() {
    let mut m = moods();
    assert_eq!(m.step(sig(false, false, false, true), true, T + 10), Mood::Celebrate);
    let mut m = moods();
    assert_eq!(m.step(sig(true, false, false, false), true, T + 10), Mood::Alert);
    let mut m = moods();
    assert_eq!(m.step(sig(false, true, false, false), true, T + 10), Mood::Error);
}

#[test]
fn the_celebration_lasts_celebrate_ms() {
    let mut m = moods();
    let t = T + 5_000;
    assert_eq!(m.step(QUIET, true, t), Mood::Celebrate);
    assert_eq!(m.step(QUIET, false, t + 1), Mood::Celebrate);
    assert_eq!(m.step(QUIET, false, t + CELEBRATE_MS - 1), Mood::Celebrate);
    assert_eq!(m.step(QUIET, false, t + CELEBRATE_MS), Mood::Idle, "exactly CELEBRATE_MS after: over");
    assert_eq!(m.step(QUIET, false, t + CELEBRATE_MS + 1), Mood::Idle);
}

#[test]
fn the_celebration_falls_back_to_the_row_below_when_it_ends() {
    let mut m = moods();
    let t = T + 1_000;
    assert_eq!(m.step(QUIET, true, t), Mood::Celebrate);
    assert_eq!(m.step(sig(false, false, true, true), false, t + 1_000), Mood::Celebrate, "working does not end it");
    assert_eq!(m.step(sig(false, false, true, true), false, t + CELEBRATE_MS), Mood::Working);
    let mut m = moods();
    assert_eq!(m.step(QUIET, true, t), Mood::Celebrate);
    assert_eq!(m.step(sig(false, false, false, true), false, t + CELEBRATE_MS), Mood::Listening);
}

#[test]
fn a_celebration_under_a_higher_row_still_ends_at_its_own_time() {
    // finished while a session needs you: alert now, the celebration shows once the alert clears, if it has not ended.
    let mut m = moods();
    let t = T + 1_000;
    assert_eq!(m.step(sig(true, false, false, false), true, t), Mood::Alert);
    assert_eq!(m.step(sig(true, false, false, false), false, t + 1_000), Mood::Alert);
    assert_eq!(m.step(QUIET, false, t + 2_000), Mood::Celebrate, "alert cleared with 2 s of the celebration left");
    assert_eq!(m.step(QUIET, false, t + CELEBRATE_MS), Mood::Idle);

    // the higher row outlasts the celebration: nothing is left of it when the row clears
    let mut m = moods();
    assert_eq!(m.step(sig(true, false, false, false), true, t), Mood::Alert);
    assert_eq!(m.step(sig(true, false, false, false), false, t + 3_999), Mood::Alert);
    assert_eq!(m.step(QUIET, false, t + CELEBRATE_MS + 1_000), Mood::Idle);

    // the same under error
    let mut m = moods();
    assert_eq!(m.step(sig(false, true, false, false), true, t), Mood::Error);
    assert_eq!(m.step(QUIET, false, t + 3_000), Mood::Celebrate);
    assert_eq!(m.step(QUIET, false, t + 4_000), Mood::Idle);
}

#[test]
fn a_second_finish_restarts_the_celebration() {
    let mut m = moods();
    let t = T + 1_000;
    assert_eq!(m.step(QUIET, true, t), Mood::Celebrate);
    assert_eq!(m.step(QUIET, true, t + 3_000), Mood::Celebrate);
    assert_eq!(m.step(QUIET, false, t + 3_000 + CELEBRATE_MS - 1), Mood::Celebrate);
    assert_eq!(m.step(QUIET, false, t + 3_000 + CELEBRATE_MS), Mood::Idle);
}

#[test]
fn a_new_avatar_is_quiet_from_new() {
    let mut m = moods();
    assert_eq!(m.step(QUIET, false, T), Mood::Idle);
    assert_eq!(m.step(QUIET, false, T + DROWSY_MS - 1), Mood::Idle);
}

#[test]
fn drowsy_at_exactly_drowsy_ms_and_sleeping_at_exactly_sleep_ms() {
    let mut m = moods();
    assert_eq!(m.step(QUIET, false, T + DROWSY_MS - 1), Mood::Idle);
    assert_eq!(m.step(QUIET, false, T + DROWSY_MS), Mood::Drowsy);
    assert_eq!(m.step(QUIET, false, T + DROWSY_MS + 1), Mood::Drowsy);
    assert_eq!(m.step(QUIET, false, T + SLEEP_MS - 1), Mood::Drowsy);
    assert_eq!(m.step(QUIET, false, T + SLEEP_MS), Mood::Sleeping);
    assert_eq!(m.step(QUIET, false, T + SLEEP_MS + 1), Mood::Sleeping);
    assert_eq!(m.step(QUIET, false, T + 10 * SLEEP_MS), Mood::Sleeping);
}

#[test]
fn a_single_late_step_lands_on_the_right_mood() {
    let mut m = moods();
    assert_eq!(m.step(QUIET, false, T + SLEEP_MS + 5), Mood::Sleeping);
    let mut m = moods();
    assert_eq!(m.step(QUIET, false, T + DROWSY_MS + 5), Mood::Drowsy);
}

#[test]
fn any_step_that_is_not_quiet_restarts_the_quiet_time() {
    for busy in [sig(true, false, false, false), sig(false, true, false, false), sig(false, false, true, false), sig(false, false, false, true)] {
        let mut m = moods();
        assert_eq!(m.step(QUIET, false, T + 100_000), Mood::Idle);
        m.step(busy, false, T + 110_000);
        let q = T + 115_000;
        assert_eq!(m.step(QUIET, false, q), Mood::Idle, "{busy:?}");
        assert_eq!(m.step(QUIET, false, q + DROWSY_MS - 1), Mood::Idle, "{busy:?}: not drowsy before DROWSY_MS of the new quiet time");
        assert_eq!(m.step(QUIET, false, q + DROWSY_MS), Mood::Drowsy, "{busy:?}");
        assert_eq!(m.step(QUIET, false, q + SLEEP_MS - 1), Mood::Drowsy);
        assert_eq!(m.step(QUIET, false, q + SLEEP_MS), Mood::Sleeping);
    }
}

#[test]
fn the_quiet_time_counts_from_the_first_step_that_found_it_quiet() {
    let mut m = moods();
    m.step(sig(false, false, true, false), false, T + 1_000);
    assert_eq!(m.step(QUIET, false, T + 10_000), Mood::Idle);
    // further quiet steps do not move the start
    assert_eq!(m.step(QUIET, false, T + 50_000), Mood::Idle);
    assert_eq!(m.step(QUIET, false, T + 10_000 + DROWSY_MS - 1), Mood::Idle);
    assert_eq!(m.step(QUIET, false, T + 10_000 + DROWSY_MS), Mood::Drowsy);
}

#[test]
fn a_celebration_is_not_quiet_so_the_quiet_time_starts_when_it_is_over() {
    let mut m = moods();
    let t = T + 10_000;
    assert_eq!(m.step(QUIET, true, t), Mood::Celebrate);
    let q = t + CELEBRATE_MS;
    assert_eq!(m.step(QUIET, false, q), Mood::Idle);
    assert_eq!(m.step(QUIET, false, q + DROWSY_MS - 1), Mood::Idle);
    assert_eq!(m.step(QUIET, false, q + DROWSY_MS), Mood::Drowsy);
}

#[test]
fn a_sleeping_avatar_wakes_on_any_condition_and_can_fall_asleep_again() {
    let mut m = moods();
    assert_eq!(m.step(QUIET, false, T + SLEEP_MS), Mood::Sleeping);
    assert_eq!(m.step(sig(false, false, true, false), false, T + SLEEP_MS + 1_000), Mood::Working);
    let q = T + SLEEP_MS + 2_000;
    assert_eq!(m.step(QUIET, false, q), Mood::Idle);
    assert_eq!(m.step(QUIET, false, q + DROWSY_MS), Mood::Drowsy);
    assert_eq!(m.step(QUIET, false, q + SLEEP_MS), Mood::Sleeping);
}

#[test]
fn a_drowsy_avatar_wakes_when_a_track_plays() {
    let mut m = moods();
    assert_eq!(m.step(QUIET, false, T + DROWSY_MS + 1_000), Mood::Drowsy);
    assert_eq!(m.step(sig(false, false, false, true), false, T + DROWSY_MS + 2_000), Mood::Listening);
}

// --- next_change ---------------------------------------------------------------------------------------------------

#[test]
fn next_change_of_a_new_avatar_is_the_drowsy_time() {
    assert_eq!(moods().next_change(), Some(T + DROWSY_MS));
}

#[test]
fn next_change_while_quiet_is_the_next_of_drowsy_and_sleep() {
    let mut m = moods();
    m.step(QUIET, false, T + 1_000);
    assert_eq!(m.next_change(), Some(T + DROWSY_MS));
    m.step(QUIET, false, T + DROWSY_MS - 1);
    assert_eq!(m.next_change(), Some(T + DROWSY_MS));
    m.step(QUIET, false, T + DROWSY_MS);
    assert_eq!(m.next_change(), Some(T + SLEEP_MS), "the drowsy time is not later than the last step");
    m.step(QUIET, false, T + SLEEP_MS - 1);
    assert_eq!(m.next_change(), Some(T + SLEEP_MS));
}

#[test]
fn next_change_is_none_while_sleeping() {
    let mut m = moods();
    m.step(QUIET, false, T + SLEEP_MS);
    assert_eq!(m.next_change(), None);
    m.step(QUIET, false, T + SLEEP_MS + 60_000);
    assert_eq!(m.next_change(), None);
}

#[test]
fn next_change_is_the_celebration_end() {
    let mut m = moods();
    let t = T + 20_000;
    m.step(QUIET, true, t);
    assert_eq!(m.next_change(), Some(t + CELEBRATE_MS));
    m.step(QUIET, false, t + 1_000);
    assert_eq!(m.next_change(), Some(t + CELEBRATE_MS));
}

#[test]
fn next_change_after_the_celebration_has_ended_is_the_new_quiet_times() {
    let mut m = moods();
    let t = T + 20_000;
    m.step(QUIET, true, t);
    m.step(QUIET, false, t + CELEBRATE_MS);
    assert_eq!(m.next_change(), Some(t + CELEBRATE_MS + DROWSY_MS));
}

#[test]
fn next_change_is_the_celebration_end_even_under_a_higher_row() {
    let mut m = moods();
    let t = T + 20_000;
    assert_eq!(m.step(sig(true, false, false, false), true, t), Mood::Alert);
    assert_eq!(m.next_change(), Some(t + CELEBRATE_MS));
}

#[test]
fn next_change_is_none_when_busy_with_no_celebration() {
    for busy in [sig(true, false, false, false), sig(false, true, false, false), sig(false, false, true, false), sig(false, false, false, true)] {
        let mut m = moods();
        m.step(busy, false, T + 1_000);
        assert_eq!(m.next_change(), None, "{busy:?}: not quiet, nothing to time");
    }
}

#[test]
fn next_change_is_none_once_the_celebration_has_passed_and_the_avatar_is_busy() {
    let mut m = moods();
    let t = T + 20_000;
    m.step(QUIET, true, t);
    m.step(sig(false, false, true, false), false, t + CELEBRATE_MS);
    assert_eq!(m.next_change(), None);
    m.step(sig(false, false, true, false), false, t + 10 * CELEBRATE_MS);
    assert_eq!(m.next_change(), None);
}

#[test]
fn next_change_after_a_busy_step_restarts_with_the_quiet_step() {
    let mut m = moods();
    m.step(sig(false, false, true, false), false, T + 30_000);
    assert_eq!(m.next_change(), None);
    m.step(QUIET, false, T + 40_000);
    assert_eq!(m.next_change(), Some(T + 40_000 + DROWSY_MS));
}

#[test]
fn next_change_is_always_later_than_the_last_step() {
    let mut m = moods();
    for now in [T, T + 1, T + DROWSY_MS - 1, T + DROWSY_MS, T + DROWSY_MS + 1, T + SLEEP_MS - 1, T + SLEEP_MS] {
        m.step(QUIET, false, now);
        if let Some(n) = m.next_change() {
            assert!(n > now, "next_change {n} is not after the step at {now}");
        }
    }
}

// --- review round 2: gaps found by mutation testing ---------------------------------------------------------------

#[test]
fn load_a_file_that_cannot_be_opened_gives_the_errors_own_text() {
    use std::os::unix::fs::PermissionsExt;
    let d = tmp("denied");
    let p = d.join("locked.json");
    fs::write(&p, text(&base())).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::File::open(&p).is_ok() {
        return; // running as root: the mode does not stop the open, so there is nothing to observe
    }
    let e = load(&p).expect_err("a file with mode 000 cannot be read");
    assert_ne!(e, "no file");
    assert_ne!(e, "not a file");
    assert!(e.to_lowercase().contains("permission denied"), "reason was {e:?}");
    fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn parse_a_repeated_key_in_a_later_array_element_names_that_index() {
    let node = |ty: &str| json!({ "surface": { "type": ty, "width": 1, "height": 1, "depth": 1, "roundness": 0 }, "position": [0, 0, 0], "rotation": [0, 0, 0] });
    let two = with("/body/nodes", json!([node("cube"), node("cube")]));
    let t = text(&two);
    let first = t.find("\"type\":\"cube\"").unwrap();
    let second = first + 1 + t[first + 1..].find("\"type\":\"cube\"").unwrap();
    let dup = format!("{}\"type\":\"cube\",{}", &t[..second], &t[second..]);
    let r = reason_of(&dup);
    assert!(r.starts_with("/body/nodes/1/surface/type: duplicate key"), "reason was {r:?}");
    let first_dup = format!("{}\"type\":\"cube\",{}", &t[..first], &t[first..]);
    let r = reason_of(&first_dup);
    assert!(r.starts_with("/body/nodes/0/surface/type: duplicate key"), "reason was {r:?}");
    // and further along: index 2
    let three = with("/body/nodes", json!([node("cube"), node("cube"), node("cube")]));
    let t = text(&three);
    let mut at = 0;
    let mut spots = vec![];
    while let Some(i) = t[at..].find("\"type\":\"cube\"") {
        spots.push(at + i);
        at += i + 1;
    }
    let third = spots[2];
    let r = reason_of(&format!("{}\"type\":\"cube\",{}", &t[..third], &t[third..]));
    assert!(r.starts_with("/body/nodes/2/surface/type: duplicate key"), "reason was {r:?}");
}

#[test]
fn parse_a_step_expression_that_is_not_a_semantic_key_is_rejected_at_its_pointer() {
    let long = key_of(65);
    for bad in [json!("Bad"), json!("a--b"), json!("-a"), json!(long), json!(5), json!(null), json!("")] {
        let r = reason_of(&text(&with("/animations/idle/steps/0/expression", bad.clone())));
        assert!(r.starts_with("/animations/idle/steps/0/expression: "), "{bad}: reason was {r:?}");
    }
}

#[test]
fn parse_an_order_entry_that_is_not_a_semantic_key_is_rejected_at_its_pointer() {
    for bad in [json!("Bad"), json!("a--b"), json!("-a"), json!(key_of(65)), json!(5), json!(null)] {
        let mut v = base();
        v["expressionOrder"] = json!(["neutral", "calm", bad.clone()]);
        let r = reason_of(&text(&v));
        assert!(r.starts_with("/expressionOrder/2: "), "{bad}: reason was {r:?}");
        let mut v = base();
        v["animationOrder"] = json!(["idle", bad.clone()]);
        let r = reason_of(&text(&v));
        assert!(r.starts_with("/animationOrder/1: "), "{bad}: reason was {r:?}");
    }
}

#[test]
fn parse_the_member_counts_of_expressions_and_animations_are_rejected_at_the_object() {
    let mut none = base();
    none["expressions"] = json!({});
    none["expressionOrder"] = json!([]);
    none["animations"] = json!({});
    none["animationOrder"] = json!([]);
    let r = reason_of(&text(&none));
    assert!(r.starts_with("/expressions: "), "reason was {r:?}");

    let mut many = base();
    let mut order = vec![json!("neutral")];
    for i in 0..128 {
        many["expressions"][format!("e{i}")] = expr(i as f64);
        order.push(json!(format!("e{i}")));
    }
    many["expressionOrder"] = Value::Array(order);
    assert_eq!(many["expressions"].as_object().unwrap().len(), 130);
    let r = reason_of(&text(&many));
    assert!(r.starts_with("/expressions: "), "130 expressions: reason was {r:?}");

    let mut exactly = base();
    let mut order = vec![json!("neutral")];
    exactly["expressions"] = json!({ "neutral": expr(0.0) });
    for i in 0..127 {
        exactly["expressions"][format!("e{i}")] = expr(i as f64);
        order.push(json!(format!("e{i}")));
    }
    exactly["expressionOrder"] = Value::Array(order);
    exactly["animations"]["idle"]["steps"][0]["expression"] = json!("e0");
    assert!(parse(&text(&exactly)).is_ok(), "128 expressions is allowed");
    let mut one_more = exactly.clone();
    one_more["expressions"]["e127"] = expr(1.0);
    one_more["expressionOrder"].as_array_mut().unwrap().push(json!("e127"));
    let r = reason_of(&text(&one_more));
    assert!(r.starts_with("/expressions: "), "129 expressions: reason was {r:?}");

    let mut anims = base();
    anims["animations"] = json!({});
    let mut order = vec![];
    for i in 0..65 {
        anims["animations"][format!("a{i}")] = animation();
        order.push(json!(format!("a{i}")));
    }
    anims["animationOrder"] = Value::Array(order);
    let r = reason_of(&text(&anims));
    assert!(r.starts_with("/animations: "), "65 animations: reason was {r:?}");
}

#[test]
fn parse_the_entry_counts_of_the_orders_are_rejected_at_the_order() {
    let r = reason_of(&text(&with("/expressionOrder", json!([]))));
    assert!(r.starts_with("/expressionOrder: "), "empty expressionOrder: reason was {r:?}");

    // 129 entries over 128 expressions: the extra one repeats a key, so only the count can be the first problem.
    let mut v = base();
    let mut order = vec![json!("neutral")];
    for i in 0..127 {
        v["expressions"][format!("e{i}")] = expr(i as f64);
        order.push(json!(format!("e{i}")));
    }
    v["expressions"].as_object_mut().unwrap().remove("calm");
    v["animations"]["idle"]["steps"][0]["expression"] = json!("e0");
    v["expressionOrder"] = Value::Array(order.clone());
    assert!(parse(&text(&v)).is_ok(), "128 entries is allowed");
    order.push(json!("e0"));
    v["expressionOrder"] = Value::Array(order);
    let r = reason_of(&text(&v));
    assert!(r.starts_with("/expressionOrder: "), "129 entries: reason was {r:?}");

    let mut a = base();
    a["animations"] = json!({});
    let mut order = vec![];
    for i in 0..64 {
        a["animations"][format!("a{i}")] = animation();
        order.push(json!(format!("a{i}")));
    }
    a["animationOrder"] = Value::Array(order.clone());
    assert!(parse(&text(&a)).is_ok(), "64 animations and entries is allowed");
    order.push(json!("a0"));
    a["animationOrder"] = Value::Array(order);
    let r = reason_of(&text(&a));
    assert!(r.starts_with("/animationOrder: "), "65 entries: reason was {r:?}");
}

#[test]
fn wait_ms_is_the_time_to_the_deadline_capped_at_max_wait_ms() {
    assert_eq!(MAX_WAIT_MS, 60_000);
    let now = T;
    assert_eq!(wait_ms(now + 600_000, now), 60_000, "10 minutes away");
    assert_eq!(wait_ms(now + 5_000, now), 5_000, "5 s away");
    assert_eq!(wait_ms(now + 60_000, now), 60_000, "exactly the cap");
    assert_eq!(wait_ms(now + 60_001, now), 60_000, "just past the cap");
    assert_eq!(wait_ms(now + 59_999, now), 59_999, "just under the cap");
    assert_eq!(wait_ms(now + 1, now), 1);
    assert_eq!(wait_ms(now, now), 0, "due now");
    assert_eq!(wait_ms(now - 1, now), 0, "1 ms late");
    assert_eq!(wait_ms(now - 600_000, now), 0, "long past");
    assert_eq!(wait_ms(0, now), 0, "no underflow");
}
