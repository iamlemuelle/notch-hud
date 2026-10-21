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
    let full = NSTemporaryDirectory() + "notch-spike-full.png"
    let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    p.arguments = ["-x", full]
    try? p.run(); p.waitUntilExit()
    guard p.terminationStatus == 0, let src = NSBitmapImageRep(data: (try? Data(contentsOf: URL(fileURLWithPath: full))) ?? Data()),
          let cropped = src.cgImage?.cropping(to: CGRect(x: r.minX * 2, y: r.minY * 2, width: r.width * 2, height: r.height * 2)) else {
        expect(false, "5. screenshot \(name) captured (needs Screen Recording permission)")
        return nil
    }
    let rep = NSBitmapImageRep(cgImage: cropped)
    try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: name))
    return rep
}
// Brightness of the pixel at global point p, inside a capture whose origin is `origin`.
func brightness(_ rep: NSBitmapImageRep, _ origin: CGPoint, _ p: CGPoint) -> CGFloat {
    rep.colorAt(x: Int((p.x - origin.x) * 2), y: Int((p.y - origin.y) * 2))?.usingColorSpace(.deviceRGB)?.brightnessComponent ?? -1
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
let left = screen.auxiliaryTopLeftArea!, right = screen.auxiliaryTopRightArea!
let notchW = right.minX - left.maxX
print("screen \(screen.frame.size) notch width \(notchW) top inset \(screen.safeAreaInsets.top)")

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
let collapsedArea = CGRect(x: b.midX - 260, y: 0, width: 520, height: 70)
if let rep = shot(collapsedArea, "shot.png") {
    let ear = CGPoint(x: left.maxX - 8, y: b.midY)
    let v = brightness(rep, collapsedArea.origin, ear)
    expect(v >= 0 && v < 0.1, "5a. pill drawn on screen: left-ear pixel at \(ear) is black (brightness \(v))")
}

let home = CGEvent(source: nil)!.location
move(CGPoint(x: b.midX, y: b.midY))
let grown = waitFor { $0.height >= 120 }
expect((grown?.height ?? 0) >= 120, "4a. hover grows height to >= 120 (got \(grown?.height ?? -1))")
if let g = grown {
    usleep(300_000)
    let area = CGRect(x: g.midX - 260, y: 0, width: 520, height: g.height + 30)
    if let rep = shot(area, "shot-expanded.png") {
        var lit = 0
        for x in stride(from: g.minX + 18, to: g.maxX - 18, by: 1) {
            for y in stride(from: g.maxY - 60, to: g.maxY - 12, by: 1) where brightness(rep, area.origin, CGPoint(x: x, y: y)) > 0.5 { lit += 1 }
        }
        expect(lit > 200, "5b. web UI reacted to hover: \(lit) bright text pixels in the detail area (> 200)")
    }
}
move(CGPoint(x: b.midX, y: 400))
let shrunk = waitFor { $0.height < 60 }
expect((shrunk?.height ?? 999) < 60, "4b. leaving shrinks it back (got \(shrunk?.height ?? -1))")
move(home)

print(failures.isEmpty ? "ALL PASS" : "FAILED: \(failures.count)")
exit(failures.isEmpty ? 0 : 1)
