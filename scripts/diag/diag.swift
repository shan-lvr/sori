// Dev diagnostics: `diag windows` lists Sori's on-screen windows; `diag fn <down|up|tap>` posts a synthetic Fn key.
import ApplicationServices
import CoreGraphics
import Foundation

let args = CommandLine.arguments
func postFn(_ down: Bool) {
    let src = CGEventSource(stateID: .hidSystemState)
    let e = CGEvent(keyboardEventSource: src, virtualKey: 63, keyDown: down)!
    e.type = .flagsChanged
    e.flags = down ? .maskSecondaryFn : []
    e.post(tap: .cghidEventTap)
}
switch args.count > 1 ? args[1] : "windows" {
case "fn":
    let what = args.count > 2 ? args[2] : "tap"
    if what == "down" { postFn(true) } else if what == "up" { postFn(false) } else {
        postFn(true); usleep(120_000); postFn(false)
    }
    print("posted fn \(what); trusted=\(AXIsProcessTrusted())")
default:
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    for w in list where (w[kCGWindowOwnerName as String] as? String) == "Sori" {
        print(w[kCGWindowNumber as String] ?? "", "layer=\(w[kCGWindowLayer as String] ?? "")",
              "bounds=\(w[kCGWindowBounds as String] ?? "")", "alpha=\(w[kCGWindowAlpha as String] ?? "")")
    }
    print("(\(list.filter { ($0[kCGWindowOwnerName as String] as? String) == "Sori" }.count) Sori windows on screen)")
}
