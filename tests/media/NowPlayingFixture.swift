// A stand-in media player for tests. It publishes one track to the system Now Playing center and keeps it
// until it receives SIGTERM, so the adapter (and the app) can be checked against known values.
//
// Usage: NowPlayingFixture <title> <artist> <square|wide> <duration-seconds> <elapsed-seconds>
// It must run inside an .app bundle: MediaRemote only reports players that have a bundle identifier.
import AppKit
import MediaPlayer

let args = CommandLine.arguments
guard args.count == 6, let duration = Double(args[4]), let elapsed = Double(args[5]),
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
center.nowPlayingInfo = [
    MPMediaItemPropertyTitle: args[1],
    MPMediaItemPropertyArtist: args[2],
    MPMediaItemPropertyAlbumTitle: "Fixture Album",
    MPMediaItemPropertyPlaybackDuration: duration,
    MPNowPlayingInfoPropertyElapsedPlaybackTime: elapsed,
    MPNowPlayingInfoPropertyPlaybackRate: 1.0,
    MPMediaItemPropertyArtwork: artwork,
]
center.playbackState = .playing

// Answering play and pause makes the fixture a real Now Playing candidate.
let commands = MPRemoteCommandCenter.shared()
commands.playCommand.addTarget { _ in center.playbackState = .playing; return .success }
commands.pauseCommand.addTarget { _ in center.playbackState = .paused; return .success }

signal(SIGTERM) { _ in exit(0) }
app.run()
