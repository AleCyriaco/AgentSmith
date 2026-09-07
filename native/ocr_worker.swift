import Foundation
import Vision
import ImageIO

struct Line: Encodable { let text: String; let confidence: Float; let x: Int; let y: Int; let width: Int; let height: Int }
struct Result: Encodable { let width: Int; let height: Int; let lines: [Line] }
do {
    // Input is a remote-session PNG, never a capture of the Mac desktop.
    let data = FileHandle.standardInput.readDataToEndOfFile()
    guard data.count <= 32 * 1024 * 1024,
          let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
          image.width <= 8192, image.height <= 8192 else { throw NSError(domain: "OCR", code: 1) }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = false
    let supported = try request.supportedRecognitionLanguages()
    request.recognitionLanguages = ["en-US", "pt-BR", "es-ES"].filter { supported.contains($0) }
    request.automaticallyDetectsLanguage = true
    try VNImageRequestHandler(cgImage: image, orientation: .up, options: [:]).perform([request])
    let w = image.width, h = image.height
    let lines = (request.results ?? []).prefix(300).compactMap { observation -> Line? in
        guard let candidate = observation.topCandidates(1).first else { return nil }
        let box = observation.boundingBox
        let x = max(0, min(w - 1, Int(floor(box.minX * Double(w)))))
        let y = max(0, min(h - 1, Int(floor((1 - box.maxY) * Double(h)))))
        let right = max(x + 1, min(w, Int(ceil(box.maxX * Double(w)))))
        let bottom = max(y + 1, min(h, Int(ceil((1 - box.minY) * Double(h)))))
        return Line(text: String(candidate.string.prefix(240)), confidence: candidate.confidence,
                    x: x, y: y, width: right - x, height: bottom - y)
    }
    FileHandle.standardOutput.write(try JSONEncoder().encode(Result(width: w, height: h, lines: lines)))
} catch {
    FileHandle.standardError.write(Data("OCR unavailable\n".utf8))
    exit(1)
}
