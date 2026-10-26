// Mouse and keyboard driver for tests/controls_test.sh and the other end-to-end tests. It posts real events and reads
// other apps' text through the accessibility API, so it needs Accessibility access.
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
//   drive type <text>               posts a key down and up per character (a-z and space only), to whatever has the keyboard
//   drive key <return|escape>       posts that key
//   drive cmd <letter>              posts Command and that letter (a-z)
//   drive paste <text>              puts <text> on the general pasteboard, posts Command-V, then puts back what the
//                                   pasteboard held before; exit 1 when it could not put it back exactly
//   drive axtext <pid> <name>       prints the text of the app's window whose document file is named <name>
//   drive axclose <pid> <name>      presses that window's close button
//   drive quit <pid>                asks the app to quit, as Quit in its menu does
//
// Exit codes: 0 done, 1 the expectation failed, 2 the environment cannot run the test.
import AppKit
import CoreGraphics

setvbuf(stdout, nil, _IOLBF, 0)

func out(_ s: String) { FileHandle.standardOutput.write((s + "\n").data(using: .utf8)!) }
// What `paste` took off the general pasteboard, until it is put back. Every way out of this process puts it back.
var pasteboardToRestore: [[NSPasteboard.PasteboardType: Data]]? = nil
func restorePasteboard() {
    guard let saved = pasteboardToRestore else { return }
    pasteboardToRestore = nil
    NSPasteboard.general.clearContents()
    NSPasteboard.general.writeObjects(saved.map { m in
        let it = NSPasteboardItem()
        for (t, d) in m { it.setData(d, forType: t) }
        return it
    })
}
func die(_ code: Int32, _ s: String) -> Never {
    restorePasteboard()
    FileHandle.standardError.write((s + "\n").data(using: .utf8)!)
    exit(code)
}

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
    // The cursor can be a small WindowServer window at the cursor level (seen while a full-screen screencaptureui
    // overlay was up); a click goes through it.
    let cursorLevel = Int(CGWindowLevelForKey(.cursorWindow))
    guard let top = windows().first(where: { $0.bounds.contains(p) && $0.layer != cursorLevel }) else { die(2, "no window under (\(p.x), \(p.y))") }
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
guard a.count >= 2 else { die(2, "usage: drive <access|cursor|warp|move|front|watch|hover|bounds|notch|screen|click|drag|type|key|cmd|paste|axtext|axclose|quit> ...") }

func num(_ i: Int) -> Double {
    guard a.count > i else { die(2, "missing argument \(i)") }
    guard let d = Double(a[i]) else { die(2, "bad number: \(a[i])") }
    return d
}
// US ANSI virtual key codes. The event also carries the character, so another layout still types it.
let KEYS: [Character: CGKeyCode] = [
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12, "w": 13, "e": 14,
    "r": 15, "y": 16, "t": 17, "o": 31, "u": 32, "i": 34, "p": 35, "l": 37, "j": 38, "k": 40, "n": 45, "m": 46, " ": 49,
]
func postKey(_ code: CGKeyCode, _ char: Character? = nil, flags: CGEventFlags = []) {
    let src = CGEventSource(stateID: .hidSystemState)
    for down in [true, false] {
        guard let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down) else { die(2, "cannot create a key event") }
        e.flags = flags
        // A Command chord keeps the layout's own character: with one set by hand, Command-V pasted nothing into
        // TextEdit (measured 2026-10-26), so Command-V, -H, -W and -Q never acted.
        if let c = char, !flags.contains(.maskCommand) {
            let u = Array(String(c).utf16)
            e.keyboardSetUnicodeString(stringLength: u.count, unicodeString: u)
        }
        e.post(tap: .cghidEventTap)
        pump(0.03)
    }
}

// The window of app `pid` whose AXDocument (a file URL) names a file called `name`.
func axWindow(_ pid: Int32, _ name: String) -> AXUIElement? {
    let app = AXUIElementCreateApplication(pid)
    var v: CFTypeRef?
    guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &v) == .success, let wins = v as? [AXUIElement] else { return nil }
    return wins.first { w in
        var d: CFTypeRef?
        guard AXUIElementCopyAttributeValue(w, kAXDocumentAttribute as CFString, &d) == .success, let s = d as? String else { return false }
        return URL(string: s)?.lastPathComponent == name
    }
}
// The value of the first text area under `e`, depth first.
func axTextArea(_ e: AXUIElement) -> String? {
    var r: CFTypeRef?
    if AXUIElementCopyAttributeValue(e, kAXRoleAttribute as CFString, &r) == .success, (r as? String) == (kAXTextAreaRole as String) {
        var v: CFTypeRef?
        return AXUIElementCopyAttributeValue(e, kAXValueAttribute as CFString, &v) == .success ? (v as? String ?? "") : nil
    }
    var c: CFTypeRef?
    guard AXUIElementCopyAttributeValue(e, kAXChildrenAttribute as CFString, &c) == .success, let kids = c as? [AXUIElement] else { return nil }
    for k in kids { if let s = axTextArea(k) { return s } }
    return nil
}

// Every item on the general pasteboard as type -> data, so it can be put back.
func pasteboardItems() -> [[NSPasteboard.PasteboardType: Data]] {
    (NSPasteboard.general.pasteboardItems ?? []).map { item in
        var m: [NSPasteboard.PasteboardType: Data] = [:]
        for t in item.types { if let d = item.data(forType: t) { m[t] = d } }
        return m
    }
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

case "type":
    guard a.count > 2 else { die(2, "missing text") }
    for c in a[2] {
        guard let code = KEYS[c] else { die(2, "cannot type \(c)") }
        postKey(code, c)
    }
    pump(0.1)

case "key":
    guard a.count > 2, let code = ["return": CGKeyCode(36), "escape": 53][a[2]] else { die(2, "key: return or escape") }
    postKey(code)
    pump(0.1)

case "cmd":
    guard a.count > 2, a[2].count == 1, let c = a[2].first, let code = KEYS[c], c != " " else { die(2, "cmd: one letter a-z") }
    postKey(code, c, flags: .maskCommand)
    pump(0.1)

case "paste":
    guard a.count > 2 else { die(2, "missing text") }
    let pb = NSPasteboard.general
    let saved = pasteboardItems()
    var signals: [DispatchSourceSignal] = []
    for sig in [SIGINT, SIGTERM, SIGHUP] {
        signal(sig, SIG_IGN)
        let src = DispatchSource.makeSignalSource(signal: sig, queue: .main)
        src.setEventHandler { restorePasteboard(); exit(128 + sig) }
        src.resume()
        signals.append(src)
    }
    pasteboardToRestore = saved
    pb.clearContents()
    // nspasteboard.org markers: clipboard managers skip transient and concealed items, so the test string is not kept.
    let item = NSPasteboardItem()
    item.setString(a[2], forType: .string)
    item.setData(Data(), forType: NSPasteboard.PasteboardType("org.nspasteboard.TransientType"))
    item.setData(Data(), forType: NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType"))
    pb.writeObjects([item])
    postKey(9, "v", flags: .maskCommand)
    pump(0.5)
    restorePasteboard()
    if pasteboardItems() != saved { die(1, "the pasteboard did not come back as it was: \(saved.count) item(s) saved") }
    out("pasted, pasteboard restored (\(saved.count) item(s))")

case "axtext":
    guard a.count > 3 else { die(2, "usage: drive axtext <pid> <name>") }
    guard let w = axWindow(pidArg(), a[3]) else { die(1, "no window for \(a[3])") }
    guard let text = axTextArea(w) else { die(1, "no text area in the window for \(a[3])") }
    print(text, terminator: "")

case "axclose":
    guard a.count > 3 else { die(2, "usage: drive axclose <pid> <name>") }
    guard let w = axWindow(pidArg(), a[3]) else { die(1, "no window for \(a[3])") }
    var b: CFTypeRef?
    guard AXUIElementCopyAttributeValue(w, kAXCloseButtonAttribute as CFString, &b) == .success, let button = b else { die(1, "no close button") }
    guard AXUIElementPerformAction(button as! AXUIElement, kAXPressAction as CFString) == .success else { die(1, "close failed") }

case "quit":
    guard let app = NSRunningApplication(processIdentifier: pidArg()) else { exit(0) }
    _ = app.terminate()

default:
    die(2, "unknown command \(a[1])")
}
