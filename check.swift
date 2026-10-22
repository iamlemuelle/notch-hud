import AppKit
import CoreGraphics

// Usage: check-bin <pid>. Asserts brief criteria 1-4 against the live window server; exit 0 only if all hold.
let pid = Int32(CommandLine.arguments[1])!
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
func isGreen(_ c: NSColor?) -> Bool {
    guard let c = c else { return false }
    // #34d058 at 0.82 opacity over black is about r 0.17, g 0.68, b 0.23.
    return c.greenComponent > 0.5 && c.greenComponent - c.redComponent > 0.2 && c.greenComponent - c.blueComponent > 0.2
}
func describe(_ c: NSColor?) -> String {
    guard let c = c else { return "no pixel" }
    return String(format: "r %.2f g %.2f b %.2f", c.redComponent, c.greenComponent, c.blueComponent)
}
// Color of the pixel at global point p, inside a capture whose origin is `origin`; nil when out of range.
func color(_ rep: NSBitmapImageRep, _ origin: CGPoint, _ p: CGPoint) -> NSColor? {
    rep.colorAt(x: Int((p.x - origin.x) * scale), y: Int((p.y - origin.y) * scale))?.usingColorSpace(.deviceRGB)
}
func brightness(_ rep: NSBitmapImageRep, _ origin: CGPoint, _ p: CGPoint) -> CGFloat {
    color(rep, origin, p)?.brightnessComponent ?? -1
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
var wasAmber = false
let collapsedArea = CGRect(x: b.midX - 260, y: 0, width: 520, height: 70)
// The first frame paints the empty state; the seeded sessions arrive within the 1s poll.
var collapsedRep: NSBitmapImageRep? = nil
_ = until(10) {
    collapsedRep = shot(collapsedArea, "shot.png")
    guard let rep = collapsedRep else { return true }
    return isAmber(color(rep, collapsedArea.origin, CGPoint(x: b.minX + 18, y: b.midY)))
}
if let rep = collapsedRep {
    let ear = CGPoint(x: left.maxX - 8, y: b.midY)
    let v = brightness(rep, collapsedArea.origin, ear)
    expect(v >= 0 && v < 0.1, "5a. pill drawn on screen: left-ear pixel at \(ear) is black (brightness \(v))")
    // Only the page can put the green status dot here: the left ear sits 14 pt in, plus half the 8 pt dot.
    let dot = CGPoint(x: b.minX + 18, y: b.midY)
    let c = color(rep, collapsedArea.origin, dot)
    wasAmber = isAmber(c)
    expect(wasAmber, "6a. a session needs permission, so the status dot at \(dot) is amber (\(describe(c)))")
}

let home = CGEvent(source: nil)!.location
move(CGPoint(x: b.midX, y: b.midY))
// Four seeded sessions: 196 + 18 + 4 + 22 * 4 + 14.
let grown = waitFor { $0.height >= 250 }
expect(grown?.height == 320 && grown?.width == 420, "4a. hover expands to 420x320 for 4 sessions (got \(grown?.size ?? .zero))")
if let g = grown {
    usleep(300_000)
    let area = CGRect(x: g.midX - 260, y: 0, width: 520, height: g.height + 30)
    if let rep = shot(area, "shot-expanded.png") {
        var lit = 0
        for x in stride(from: g.minX + 18, to: g.maxX - 18, by: 1) {
            for y in stride(from: g.maxY - 60, to: g.maxY - 12, by: 1) where brightness(rep, area.origin, CGPoint(x: x, y: y)) > 0.5 { lit += 1 }
        }
        let samples = Int((g.width - 36) * 48)
        expect(lit > 200 && lit < samples / 4, "5b. web UI reacted to hover: \(lit) bright text pixels of \(samples) in the detail area (> 200, < 1/4)")
        let bg = brightness(rep, area.origin, CGPoint(x: g.minX + 8, y: g.maxY - 40))
        expect(bg >= 0 && bg < 0.03, "5b. expanded panel is painted black, not see-through (brightness \(bg))")
    }
}
move(CGPoint(x: b.midX, y: 400))
let shrunk = waitFor { $0.height < 60 }
expect((shrunk?.height ?? 999) < 60, "4b. leaving shrinks it back (got \(shrunk?.height ?? -1))")
move(home)

// The permission prompt in s1 is answered and the turn ends: the dot must turn green (s2 is still working).
let hookRun = Process()
hookRun.executableURL = URL(fileURLWithPath: "/bin/sh")
hookRun.arguments = ["-c", "printf '%s' '{\"session_id\":\"s1\",\"cwd\":\"/x/bianki\",\"hook_event_name\":\"Stop\"}' | ./hooks/notch-hook.sh"]
try? hookRun.run(); hookRun.waitUntilExit()
var turned = false
var last: NSColor? = nil
for _ in 0..<12 where !turned {
    usleep(250_000)
    let area = CGRect(x: b.midX - 260, y: 0, width: 520, height: 70)
    if let rep = shot(area, "shot-after-stop.png") {
        last = color(rep, area.origin, CGPoint(x: b.minX + 18, y: b.midY))
        turned = isGreen(last)
    }
}
expect(wasAmber && turned, "6b. (needs 6a) after s1's Stop goes through the hook, the dot turns green within 3s (\(describe(last)))")

print(failures.isEmpty ? "ALL PASS" : "FAILED: \(failures.count)")
exit(failures.isEmpty ? 0 : 1)
