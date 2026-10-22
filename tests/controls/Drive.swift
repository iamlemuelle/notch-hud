// Mouse driver for tests/controls_test.sh. It posts real mouse events, so it needs Accessibility (post-event) access.
// Every point is in global top-left coordinates, the same space as CGWindowListCopyWindowInfo bounds and CGEvent.
//
//   drive access                    exit 0 when this process may post events, else exit 2
//   drive cursor                    prints "<x> <y>"
//   drive warp <x> <y>              moves the cursor without a click
//   drive move <x> <y>              posts a real mouseMoved event at the point (needs Accessibility), no warp
//   drive front                     prints "<pid> <bundle id>" of the frontmost app
//   drive watch                     prints "watching", then one "activated <pid> <bundle id>" line per app activation
//   drive hover <pid> [open height] moves onto the app's pill and waits until its window is the open window, 452 wide
//                                   and <open height> + 16 tall (default 380, the detail height); prints "x0=<n>", the panel's left edge
//                                   (the window's left edge + 16), once the panel has expanded
//   drive bounds <pid>              prints "<x> <y> <w> <h>" of the app's panel window, exit 1 when it has none
//   drive notch                     prints "<width> <height>" of the notch on the notched screen
//   drive screen                    prints "<x> <y> <w> <h>" of the notched screen in global top-left coordinates
//   drive click <pid> <x> <y>       a left click
//   drive drag <pid> <x0> <x1> <y> <steps>   press at x0, drag in <steps> moves to x1, release
//
// Exit codes: 0 done, 1 the expectation failed, 2 the environment cannot run the test.
import AppKit
import CoreGraphics

setvbuf(stdout, nil, _IOLBF, 0)

func out(_ s: String) { FileHandle.standardOutput.write((s + "\n").data(using: .utf8)!) }
func die(_ code: Int32, _ s: String) -> Never { FileHandle.standardError.write((s + "\n").data(using: .utf8)!); exit(code) }

struct Win { let pid: Int32; let bounds: CGRect; let layer: Int }

// On-screen windows, front to back.
func windows() -> [Win] {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    return list.compactMap { w in
        guard let pid = w[kCGWindowOwnerPID as String] as? Int32,
              let b = w[kCGWindowBounds as String] as? [String: Any],
              let rect = CGRect(dictionaryRepresentation: b as CFDictionary),
              (w[kCGWindowAlpha as String] as? Double ?? 1) > 0 else { return nil }
        return Win(pid: pid, bounds: rect, layer: w[kCGWindowLayer as String] as? Int ?? 0)
    }
}

func panel(of pid: Int32) -> CGRect? {
    windows().first { $0.pid == pid && $0.bounds.minY.magnitude < 1 && $0.bounds.width >= 200 && $0.bounds.height <= 400 }?.bounds
}

// The test is only valid when the click lands on the test app's own window.
func requireTopmost(_ pid: Int32, at p: CGPoint) {
    guard let top = windows().first(where: { $0.bounds.contains(p) }) else { die(2, "no window under (\(p.x), \(p.y))") }
    if top.pid != pid {
        let cursor = CGEvent(source: nil)?.location ?? .zero
        die(2, "the window under (\(Int(p.x)), \(Int(p.y))) belongs to pid \(top.pid), not the test app \(pid) (its panel: \(panel(of: pid).map { "\($0)" } ?? "none"), cursor at \(cursor)): another notch-hud on top, or the panel collapsed?")
    }
}

func post(_ type: CGEventType, _ p: CGPoint) {
    guard let e = CGEvent(mouseEventSource: CGEventSource(stateID: .hidSystemState), mouseType: type, mouseCursorPosition: p, mouseButton: .left) else {
        die(2, "cannot create a mouse event")
    }
    if type != .mouseMoved { e.setIntegerValueField(.mouseEventClickState, value: 1) }
    e.post(tap: .cghidEventTap)
}

// Front-to-back owners of the windows under p, for diagnostics.
func stack(at p: CGPoint) -> String {
    windows().filter { $0.bounds.contains(p) }.prefix(3).map { "\($0.pid)@\($0.layer) \(Int($0.bounds.width))x\(Int($0.bounds.height))" }.joined(separator: " > ")
}

func notched() -> NSScreen? { NSScreen.screens.first { $0.safeAreaInsets.top > 0 } }

func pump(_ s: Double) { RunLoop.current.run(until: Date().addingTimeInterval(s)) }

let a = CommandLine.arguments
guard a.count >= 2 else { die(2, "usage: drive <access|cursor|warp|move|front|watch|hover|bounds|notch|screen|click|drag> ...") }

func num(_ i: Int) -> Double {
    guard a.count > i else { die(2, "missing argument \(i)") }
    guard let d = Double(a[i]) else { die(2, "bad number: \(a[i])") }
    return d
}
func pidArg() -> Int32 {
    guard a.count > 2 else { die(2, "missing pid") }
    guard let p = Int32(a[2]) else { die(2, "bad pid: \(a[2])") }
    return p
}

switch a[1] {
case "access":
    exit(CGPreflightPostEventAccess() ? 0 : 2)

case "cursor":
    let p = CGEvent(source: nil)?.location ?? .zero
    out("\(p.x) \(p.y)")

case "warp":
    CGWarpMouseCursorPosition(CGPoint(x: num(2), y: num(3)))

case "move":
    post(.mouseMoved, CGPoint(x: num(2), y: num(3)))
    pump(0.05)

case "front":
    pump(0.2)
    if let f = NSWorkspace.shared.frontmostApplication { out("\(f.processIdentifier) \(f.bundleIdentifier ?? "?")") } else { out("0 none") }

case "watch":
    NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main) { n in
        let app = n.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
        out("activated \(app?.processIdentifier ?? 0) \(app?.bundleIdentifier ?? "?")")
    }
    out("watching")
    RunLoop.main.run()

case "bounds":
    guard let r = panel(of: pidArg()) else { exit(1) }
    out("\(Int(r.minX.rounded())) \(Int(r.minY.rounded())) \(Int(r.width.rounded())) \(Int(r.height.rounded()))")

case "notch":
    guard let s = notched() else { die(2, "no notched screen") }
    guard let left = s.auxiliaryTopLeftArea, let right = s.auxiliaryTopRightArea else { die(2, "no auxiliary top areas") }
    out("\(Int((right.minX - left.maxX).rounded())) \(Int(s.safeAreaInsets.top.rounded()))")

case "screen":
    guard let s = notched(), let id = s.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID else { die(2, "no notched screen") }
    let b = CGDisplayBounds(id)
    out("\(Int(b.minX)) \(Int(b.minY)) \(Int(b.width)) \(Int(b.height))")

case "hover":
    let pid = pidArg()
    let wantHeight = (a.count > 3 ? num(3) : 380) + 16
    var seen: CGRect? = nil
    for _ in 0..<50 { if let r = panel(of: pid) { seen = r; break }; pump(0.1) }
    guard let collapsed = seen else { die(2, "no panel window found for pid \(pid)") }
    let target = CGPoint(x: collapsed.midX, y: collapsed.minY + 10)
    requireTopmost(pid, at: target)
    var last = collapsed
    for _ in 0..<50 {
        post(.mouseMoved, target)
        pump(0.1)
        if let r = panel(of: pid) {
            last = r
            if abs(r.width - 452) < 0.5 && abs(r.height - wantHeight) < 0.5 {
                // The window grows when the 120 ms peek starts, while the hit rect is still the peek shape: wait for
                // the expand (CONTRACT.md section 8), or a move down to the controls counts as leaving.
                pump(0.3)
                out("x0=\(Int((r.minX + 16).rounded()))")
                exit(0)
            }
        }
    }
    die(1, "window did not reach 452 x \(Int(wantHeight)) on hover; last bounds \(last)")

case "click":
    let pid = pidArg()
    let p = CGPoint(x: num(3), y: num(4))
    requireTopmost(pid, at: p)
    post(.mouseMoved, p); pump(0.15)
    post(.leftMouseDown, p); pump(0.06)
    post(.leftMouseUp, p); pump(0.1)

case "drag":
    let pid = pidArg()
    let x0 = num(3), x1 = num(4), y = num(5), steps = max(10, Int(num(6)))
    requireTopmost(pid, at: CGPoint(x: x0, y: y))
    post(.mouseMoved, CGPoint(x: x0, y: y)); pump(0.15)
    let start = CGPoint(x: x0, y: y), end = CGPoint(x: x1, y: y)
    FileHandle.standardError.write("drag: before down: \(stack(at: start))\n".data(using: .utf8)!)
    post(.leftMouseDown, start); pump(0.08)
    FileHandle.standardError.write("drag: after down: \(stack(at: start))\n".data(using: .utf8)!)
    for i in 1...steps {
        post(.leftMouseDragged, CGPoint(x: x0 + (x1 - x0) * Double(i) / Double(steps), y: y))
        pump(0.03)
    }
    pump(0.08)
    FileHandle.standardError.write("drag: before up: \(stack(at: end))\n".data(using: .utf8)!)
    post(.leftMouseUp, end); pump(0.1)

default:
    die(2, "unknown command \(a[1])")
}
