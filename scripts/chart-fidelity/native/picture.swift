import AppKit
import Foundation

// Collect only the picture produced by the copy operation; restore the pasteboard afterward.
let args = Array(CommandLine.arguments.dropFirst())
guard args.count == 6 else { exit(2) }
let destination = URL(fileURLWithPath: args[0])
let board = NSPasteboard.general
let prior = (board.pasteboardItems ?? []).map { item in
    item.types.compactMap { type in item.data(forType: type).map { (type, $0) } }
}
let start = board.changeCount
let command = Process()
command.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
command.arguments = Array(args.dropFirst())
try command.run()
command.waitUntilExit()
guard command.terminationStatus == 0 else { exit(1) }
for _ in 0..<20 {
    if board.changeCount != start && board.data(forType: .pdf) != nil { break }
    Thread.sleep(forTimeInterval: 0.1)
}
guard board.changeCount != start else { exit(1) }
let owned = board.changeCount
defer {
    if board.changeCount == owned {
        board.clearContents()
        let items = prior.map { values -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for (key, data) in values { item.setData(data, forType: key) }
            return item
        }
        if !items.isEmpty { board.writeObjects(items) }
    }
}
if let pdf = board.data(forType: .pdf) {
    try pdf.write(to: destination)
} else if let nativeImage = NSImage(pasteboard: board),
          let tiff = nativeImage.tiffRepresentation,
          let raster = NSBitmapImageRep(data: tiff),
          let png = raster.representation(using: .png, properties: [:]) {
    try png.write(to: destination.deletingPathExtension().appendingPathExtension("png"))
} else {
    throw NSError(domain: "OfficeChartPicture", code: 1)
}
