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
func shot(_ r: CGRect, _ name: String) {
    let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    p.arguments = ["-x", "-R\(Int(r.minX)),\(Int(r.minY)),\(Int(r.width)),\(Int(r.height))", name]
    try? p.run(); p.waitUntilExit()
    expect(FileManager.default.fileExists(atPath: name), "5. screenshot saved to \(name) (needs Screen Recording permission)")
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
shot(CGRect(x: b.midX - 260, y: 0, width: 520, height: 70), "shot.png")

let home = CGEvent(source: nil)!.location
move(CGPoint(x: b.midX, y: b.midY))
let grown = waitFor { $0.height >= 120 }
expect((grown?.height ?? 0) >= 120, "4a. hover grows height to >= 120 (got \(grown?.height ?? -1))")
if let g = grown { shot(CGRect(x: g.midX - 260, y: 0, width: 520, height: g.height + 30), "shot-expanded.png") }
move(CGPoint(x: b.midX, y: 400))
let shrunk = waitFor { $0.height < 60 }
expect((shrunk?.height ?? 999) < 60, "4b. leaving shrinks it back (got \(shrunk?.height ?? -1))")
move(home)

print(failures.isEmpty ? "ALL PASS" : "FAILED: \(failures.count)")
exit(failures.isEmpty ? 0 : 1)
