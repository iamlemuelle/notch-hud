// A stand-in media player for tests. It publishes one track to the system Now Playing center and keeps it
// until it receives SIGTERM, so the adapter (and the app) can be checked against known values.
// It also answers MediaRemote's transport commands and prints one line to stdout per command:
//   fixture: toggle playing | toggle paused | play | pause | next "<title> II" | previous "<title>" | seek <s>
//
// Usage: NowPlayingFixture <title> <artist> <square|wide> <duration-seconds> <elapsed-seconds>
// It must run inside an .app bundle: MediaRemote only reports players that have a bundle identifier.
import AppKit
import MediaPlayer

let args = CommandLine.arguments
guard args.count == 6, let duration = Double(args[4]), let startElapsed = Double(args[5]),
      args[3] == "square" || args[3] == "wide" else {
    FileHandle.standardError.write("usage: NowPlayingFixture <title> <artist> <square|wide> <duration> <elapsed>\n".data(using: .utf8)!)
    exit(2)
}

let size = args[3] == "wide" ? NSSize(width: 320, height: 180) : NSSize(width: 256, height: 256)
let image = NSImage(size: size, flipped: false) { rect in
    NSColor(calibratedRed: 0.31, green: 0.49, blue: 1.0, alpha: 1).setFill()
    rect.fill()
    NSColor.white.setFill()
    NSBezierPath(ovalIn: NSRect(x: rect.midX - 20, y: rect.midY - 20, width: 40, height: 40)).fill()
    return true
}
let artwork = MPMediaItemArtwork(boundsSize: size) { _ in image }

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let center = MPNowPlayingInfoCenter.default()

// The fixture's whole state. Every command changes it and republishes it, so the adapter stream reports it.
let baseTitle = args[1]
var title = baseTitle
var position = startElapsed
var playing = true
var anchor = Date()

func say(_ line: String) {
    FileHandle.standardOutput.write((line + "\n").data(using: .utf8)!)
}

func publish() {
    center.nowPlayingInfo = [
        MPMediaItemPropertyTitle: title,
        MPMediaItemPropertyArtist: args[2],
        MPMediaItemPropertyAlbumTitle: "Fixture Album",
        MPMediaItemPropertyPlaybackDuration: duration,
        MPNowPlayingInfoPropertyElapsedPlaybackTime: position,
        MPNowPlayingInfoPropertyPlaybackRate: playing ? 1.0 : 0.0,
        MPMediaItemPropertyArtwork: artwork,
    ]
    center.playbackState = playing ? .playing : .paused
}

// Brings `position` up to now before a command that changes the rate: it moves on while playing.
func settle() {
    if playing { position = min(duration, position + Date().timeIntervalSince(anchor)) }
    anchor = Date()
}

publish()

let commands = MPRemoteCommandCenter.shared()
commands.togglePlayPauseCommand.addTarget { _ in
    settle(); playing.toggle(); publish()
    say("fixture: toggle " + (playing ? "playing" : "paused"))
    return .success
}
commands.playCommand.addTarget { _ in
    settle(); playing = true; publish(); say("fixture: play"); return .success
}
commands.pauseCommand.addTarget { _ in
    settle(); playing = false; publish(); say("fixture: pause"); return .success
}
commands.nextTrackCommand.addTarget { _ in
    title = baseTitle + " II"; position = 0; playing = true; anchor = Date(); publish()
    say("fixture: next \"\(title)\""); return .success
}
commands.previousTrackCommand.addTarget { _ in
    title = baseTitle; position = 0; playing = true; anchor = Date(); publish()
    say("fixture: previous \"\(title)\""); return .success
}
commands.changePlaybackPositionCommand.addTarget { event in
    guard let e = event as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
    position = e.positionTime; anchor = Date(); publish()
    say(String(format: "fixture: seek %.1f", e.positionTime)); return .success
}
for c in [commands.togglePlayPauseCommand, commands.playCommand, commands.pauseCommand, commands.nextTrackCommand,
          commands.previousTrackCommand, commands.changePlaybackPositionCommand] as [MPRemoteCommand] {
    c.isEnabled = true
}

signal(SIGTERM) { _ in exit(0) }
app.run()
