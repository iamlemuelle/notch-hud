import AppKit
import CoreGraphics

// Usage: check-bin <pid> <trace file>. Asserts brief criteria 1-4 against the live window server and the app's
// $NOTCH_TRACE lines, which check.sh sends to <trace file>; exit 0 only if all hold.
let pid = Int32(CommandLine.arguments[1])!
let tracePath = CommandLine.arguments[2]
var failures: [String] = []
func expect(_ ok: Bool, _ msg: String) { print((ok ? "PASS " : "FAIL ") + msg); if !ok { failures.append(msg) } }

func windows() -> [[String: Any]] {
    let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
    return all.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid }
}
func bounds(_ w: [String: Any]) -> CGRect {
    CGRect(dictionaryRepresentation: w[kCGWindowBounds as String] as! CFDictionary)!
}
// Region capture (-R) leaves out windows above the menu bar on this macOS, so capture the whole display and crop.
func shot(_ r: CGRect, _ name: String) -> NSBitmapImageRep? {
    try? FileManager.default.removeItem(atPath: name)
    let full = NSTemporaryDirectory() + "notch-hud-full.png"
    try? FileManager.default.removeItem(atPath: full)
    let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    p.arguments = ["-x", full]
    try? p.run(); p.waitUntilExit()
    guard p.terminationStatus == 0, let src = NSBitmapImageRep(data: (try? Data(contentsOf: URL(fileURLWithPath: full))) ?? Data()),
          let cropped = src.cgImage?.cropping(to: CGRect(x: r.minX * scale, y: r.minY * scale, width: r.width * scale, height: r.height * scale)) else {
        expect(false, "5. screenshot \(name) captured (needs Screen Recording permission)")
        return nil
    }
    let rep = NSBitmapImageRep(cgImage: cropped)
    try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: name))
    return rep
}
func isAmber(_ c: NSColor?) -> Bool {
    guard let c = c else { return false }
    // #f5a623 at 0.82 opacity over black is about r 0.79, g 0.53, b 0.11.
    return c.redComponent > 0.65 && c.greenComponent > 0.4 && c.greenComponent < 0.7 && c.blueComponent < 0.3
}
func isText(_ c: NSColor?) -> Bool {
    guard let c = c else { return false }
    // #f5f5f5 at 0.82 opacity over black is about 0.79 on every channel (0.96 at opacity 1, while peeking).
    let r = c.redComponent, g = c.greenComponent, b = c.blueComponent
    return min(r, g, b) > 0.6 && max(r, g, b) - min(r, g, b) < 0.08
}
// Color of the pixel at global point p, inside a capture whose origin is `origin`; nil when out of range.
func color(_ rep: NSBitmapImageRep, _ origin: CGPoint, _ p: CGPoint) -> NSColor? {
    rep.colorAt(x: Int((p.x - origin.x) * scale), y: Int((p.y - origin.y) * scale))?.usingColorSpace(.deviceRGB)
}
func brightness(_ rep: NSBitmapImageRep, _ origin: CGPoint, _ p: CGPoint) -> CGFloat {
    color(rep, origin, p)?.brightnessComponent ?? -1
}
func trace() -> [String] {
    ((try? String(contentsOfFile: tracePath, encoding: .utf8)) ?? "").split(separator: "\n").map(String.init)
}
// `attention` of the last `sessions:` summary in these lines (CONTRACT.md section 3); nil when there is none.
func attention(_ lines: ArraySlice<String>) -> Int? {
    guard let l = lines.last(where: { $0.hasPrefix("sessions: ") }),
          let s = try? JSONSerialization.jsonObject(with: Data(l.dropFirst(10).utf8)) as? [String: Any] else { return nil }
    return s["attention"] as? Int
}
// The `avatar: <mood>` lines in order (CONTRACT.md section 10), leaving out `avatar: loaded` and load failures.
func moods(_ lines: ArraySlice<String>) -> [String] {
    let all: Set<String> = ["alert", "error", "celebrate", "working", "listening", "idle", "drowsy", "sleeping"]
    return lines.compactMap { $0.hasPrefix("avatar: ") ? String($0.dropFirst(8)) : nil }.filter(all.contains)
}
func move(_ pt: CGPoint) {
    CGWarpMouseCursorPosition(pt)
    CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: pt, mouseButton: .left)?.post(tap: .cghidEventTap)
}
func waitFor(_ cond: (CGRect) -> Bool) -> CGRect? {
    for _ in 0..<15 {
        if let w = windows().first, cond(bounds(w)) { return bounds(w) }
        usleep(100_000)
    }
    return windows().first.map(bounds)
}

let screen = NSScreen.main!
let scale = screen.backingScaleFactor
let left = screen.auxiliaryTopLeftArea!, right = screen.auxiliaryTopRightArea!
let notchW = right.minX - left.maxX
print("screen \(screen.frame.size) notch width \(notchW) top inset \(screen.safeAreaInsets.top)")

// A fresh build can take a few seconds to show its window and paint: wait for the expected state, up to 10s.
func until(_ seconds: Double, _ cond: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(seconds)
    while Date() < deadline { if cond() { return true }; usleep(200_000) }
    return cond()
}
_ = until(10) { windows().count == 1 }
let ws = windows()
expect(ws.count == 1, "1. exactly one on-screen window (got \(ws.count))")
guard let w = ws.first else { exit(1) }
let layer = w[kCGWindowLayer as String] as! Int
let b = bounds(w)
print("window layer \(layer) bounds \(b)")
expect(layer > 24, "2. layer \(layer) > 24 (above menu bar)")
expect(b.minY == 0, "3a. top edge y == 0 (got \(b.minY))")
expect(abs(b.midX - screen.frame.width / 2) <= 2, "3b. centered within 2pt (midX \(b.midX) vs \(screen.frame.width / 2))")
expect(b.width > notchW, "3c. wider than notch (\(b.width) > \(notchW))")
// `#total`, the session count, has its left edge 41 pt into the pill and is centered on its height (CONTRACT.md
// section 10, "Page"); one 12 px digit fits in the 12 x 10 pt box from there. Counts its amber and #f5f5f5 pixels.
func total(_ rep: NSBitmapImageRep, _ origin: CGPoint) -> (amber: Int, text: Int) {
    var amber = 0, text = 0
    for x in stride(from: b.minX + 41, to: b.minX + 53, by: 1 / scale) {
        for y in stride(from: b.midY - 5, to: b.midY + 5, by: 1 / scale) {
            let c = color(rep, origin, CGPoint(x: x, y: y))
            if isAmber(c) { amber += 1 } else if isText(c) { text += 1 }
        }
    }
    return (amber, text)
}
var wasAmber = false
let collapsedArea = CGRect(x: b.midX - 260, y: 0, width: 520, height: 70)
// The first frame paints the empty state; the seeded sessions arrive within the 1s poll.
var collapsedRep: NSBitmapImageRep? = nil
_ = until(10) {
    collapsedRep = shot(collapsedArea, "shot.png")
    guard let rep = collapsedRep else { return true }
    return total(rep, collapsedArea.origin).amber >= 6
}
if let rep = collapsedRep {
    let ear = CGPoint(x: left.maxX - 8, y: b.midY)
    let v = brightness(rep, collapsedArea.origin, ear)
    expect(v >= 0 && v < 0.1, "5a. pill drawn on screen: left-ear pixel at \(ear) is black (brightness \(v))")
    // Only the page can color the count: amber while a session needs you, else #f5f5f5 (CONTRACT.md section 4).
    let t = total(rep, collapsedArea.origin)
    wasAmber = t.amber >= 6
    expect(wasAmber, "6a. a session needs permission, so the count is amber (\(t.amber) amber, \(t.text) #f5f5f5 pixels; want >= 6 amber)")
}
// s1 needs permission and no seeded session is an error or needs input, so attention is 1 and the mood is alert.
_ = until(3) { attention(trace()[...]) == 1 && moods(trace()[...]).last == "alert" }
let seeded = trace()[...]
expect(attention(seeded) == 1, "6a. the last traced summary has attention 1 (got \(attention(seeded).map(String.init) ?? "none"))")
expect(moods(seeded).last == "alert", "6a. the last traced mood is avatar: alert (got \(moods(seeded).last ?? "none"))")

let home = CGEvent(source: nil)!.location
move(CGPoint(x: b.midX, y: b.midY))
// Four seeded sessions: the open window is 452 wide (420 + 2 * 16) and max(open, detail) + 16 = 396 tall (CONTRACT.md
// section 8, Detail); the open shape is 196 + 18 + 4 + 22 * 4 + 14 = 320 tall, so it ends above the window's bottom.
let grown = waitFor { $0.height >= 250 }
expect(grown?.height == 396 && grown?.width == 452, "4a. hover grows the window to 452x396 for 4 sessions (got \(grown?.size ?? .zero))")
if let g = grown {
    usleep(300_000)
    let shapeBottom = g.minY + 320
    let area = CGRect(x: g.midX - 260, y: 0, width: 520, height: g.height + 30)
    if let rep = shot(area, "shot-expanded.png") {
        var lit = 0
        for x in stride(from: g.minX + 34, to: g.maxX - 34, by: 1) {
            for y in stride(from: shapeBottom - 60, to: shapeBottom - 12, by: 1) where brightness(rep, area.origin, CGPoint(x: x, y: y)) > 0.5 { lit += 1 }
        }
        let samples = Int((g.width - 68) * 48)
        expect(lit > 200 && lit < samples / 4, "5b. web UI reacted to hover: \(lit) bright text pixels of \(samples) in the detail area (> 200, < 1/4)")
        let bg = brightness(rep, area.origin, CGPoint(x: g.minX + 24, y: shapeBottom - 40))
        expect(bg >= 0 && bg < 0.03, "5b. expanded panel is painted black, not see-through (brightness \(bg))")
    }
}
move(CGPoint(x: b.midX, y: 400))
let shrunk = waitFor { $0.height < 60 }
expect((shrunk?.height ?? 999) < 60, "4b. leaving shrinks it back (got \(shrunk?.height ?? -1))")
move(home)

// The permission prompt in s1 is answered and the turn ends: attention drops to 0, so the count turns #f5f5f5, and s1
// just finished, so the avatar celebrates.
let mark = trace().count
let hookRun = Process()
hookRun.executableURL = URL(fileURLWithPath: "/bin/sh")
hookRun.arguments = ["-c", "printf '%s' '{\"session_id\":\"s1\",\"cwd\":\"/x/bianki\",\"hook_event_name\":\"Stop\"}' | ./hooks/notch-hook.sh"]
try? hookRun.run(); hookRun.waitUntilExit()
var turned = false
var last = (amber: -1, text: -1)
for _ in 0..<12 where !turned {
    usleep(250_000)
    let area = CGRect(x: b.midX - 260, y: 0, width: 520, height: 70)
    if let rep = shot(area, "shot-after-stop.png") {
        last = total(rep, area.origin)
        turned = last.amber == 0 && last.text >= 6
    }
}
expect(wasAmber && turned, "6b. (needs 6a) after s1's Stop goes through the hook, the count turns #f5f5f5 within 3s (\(last.amber) amber, \(last.text) #f5f5f5 pixels; want 0 and >= 6)")
_ = until(3) { attention(trace()[mark...]) == 0 && moods(trace()[mark...]).contains("celebrate") }
let stopped = trace()[mark...]
expect(attention(stopped) == 0, "6b. after the Stop the traced summary has attention 0 (got \(attention(stopped).map(String.init) ?? "none"))")
expect(moods(stopped).contains("celebrate"), "6b. after the Stop the avatar traces celebrate (got \(moods(stopped)))")

print(failures.isEmpty ? "ALL PASS" : "FAILED: \(failures.count)")
exit(failures.isEmpty ? 0 : 1)
