// Renders TendKit's app icon to a 1024×1024 PNG following the macOS icon
// grid: an 824 pt rounded-square body, centered, with a soft drop shadow.
// Usage: swift scripts/make-icon.swift [--dev] <output.png>
// --dev adds the corner badge that marks development builds.
import AppKit

let size: CGFloat = 1024
let arguments = CommandLine.arguments.dropFirst()
let developer = arguments.contains("--dev")
let output = arguments.first { $0 != "--dev" } ?? "AppIcon.png"

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> NSColor {
  NSColor(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255,
    blue: CGFloat(hex & 0xFF) / 255, alpha: alpha)
}

/// Superellipse approximating Apple's continuous-corner icon shape.
func squircle(_ rect: CGRect, exponent: CGFloat = 5) -> NSBezierPath {
  let path = NSBezierPath()
  let (a, b) = (rect.width / 2, rect.height / 2)
  for step in 0...720 {
    let t = CGFloat(step) / 720 * 2 * .pi
    let (c, s) = (cos(t), sin(t))
    let x = pow(abs(c), 2 / exponent) * a * (c < 0 ? -1 : 1)
    let y = pow(abs(s), 2 / exponent) * b * (s < 0 ? -1 : 1)
    let point = CGPoint(x: rect.midX + x, y: rect.midY + y)
    step == 0 ? path.move(to: point) : path.line(to: point)
  }
  path.close()
  return path
}

/// A pointed leaf from `base` to `tip`; `bulge` is the half-width at the middle
/// and `lean` shifts the widest point toward the base (negative) or the tip.
func leaf(from base: CGPoint, to tip: CGPoint, bulge: CGFloat, lean: CGFloat = 0) -> NSBezierPath {
  let (dx, dy) = (tip.x - base.x, tip.y - base.y)
  let length = hypot(dx, dy)
  let (ux, uy) = (dx / length, dy / length)
  let (nx, ny) = (-uy, ux)
  func point(_ along: CGFloat, _ across: CGFloat) -> CGPoint {
    CGPoint(x: base.x + ux * length * along + nx * across, y: base.y + uy * length * along + ny * across)
  }
  let path = NSBezierPath()
  path.move(to: base)
  path.curve(to: tip, controlPoint1: point(0.15 + lean, bulge * 1.5), controlPoint2: point(0.75 + lean, bulge * 1.1))
  path.curve(to: base, controlPoint1: point(0.7 + lean, -bulge * 0.9), controlPoint2: point(0.2 + lean, -bulge * 1.3))
  path.close()
  return path
}

let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size),
  bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
  colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let context = NSGraphicsContext.current!.cgContext

// Body with the standard macOS icon shadow.
let body = squircle(CGRect(x: 100, y: 100, width: 824, height: 824))
NSGraphicsContext.saveGraphicsState()
let shadow = NSShadow()
shadow.shadowColor = color(0x000000, 0.28)
shadow.shadowOffset = NSSize(width: 0, height: -12)
shadow.shadowBlurRadius = 24
shadow.set()
color(0x0F7A5A).setFill()
body.fill()
NSGraphicsContext.restoreGraphicsState()

NSGraphicsContext.saveGraphicsState()
body.addClip()
NSGradient(colors: [color(0x5FD9A4), color(0x1FA37A), color(0x0B5E56)],
  atLocations: [0, 0.5, 1], colorSpace: .sRGB)!.draw(in: body, angle: -90)
// Soft top-left light so the surface reads as glass rather than flat paint.
NSGradient(colors: [color(0xFFFFFF, 0.3), color(0xFFFFFF, 0)])!
  .draw(fromCenter: CGPoint(x: 330, y: 860), radius: 0, toCenter: CGPoint(x: 330, y: 860), radius: 620, options: [])

// Soil: a low mound the seedling grows from.
let soil = NSBezierPath()
soil.move(to: CGPoint(x: 250, y: 320))
soil.curve(to: CGPoint(x: 512, y: 372), controlPoint1: CGPoint(x: 320, y: 322), controlPoint2: CGPoint(x: 410, y: 372))
soil.curve(to: CGPoint(x: 774, y: 320), controlPoint1: CGPoint(x: 614, y: 372), controlPoint2: CGPoint(x: 704, y: 322))
soil.curve(to: CGPoint(x: 250, y: 320), controlPoint1: CGPoint(x: 700, y: 262), controlPoint2: CGPoint(x: 324, y: 262))
soil.close()
NSGraphicsContext.saveGraphicsState()
let soilShadow = NSShadow()
soilShadow.shadowColor = color(0x053B33, 0.4)
soilShadow.shadowOffset = NSSize(width: 0, height: -8)
soilShadow.shadowBlurRadius = 20
soilShadow.set()
color(0xFFFFFF, 0.92).setFill()
soil.fill()
NSGraphicsContext.restoreGraphicsState()

// Seedling: a stem with a small leaf on the left and a larger one on the right.
let stem = NSBezierPath()
stem.move(to: CGPoint(x: 512, y: 352))
stem.curve(to: CGPoint(x: 520, y: 560), controlPoint1: CGPoint(x: 506, y: 430), controlPoint2: CGPoint(x: 526, y: 490))
stem.lineWidth = 34
stem.lineCapStyle = .round

let leaves = [
  leaf(from: CGPoint(x: 516, y: 500), to: CGPoint(x: 330, y: 650), bulge: 62, lean: 0.05),
  leaf(from: CGPoint(x: 520, y: 556), to: CGPoint(x: 735, y: 790), bulge: 84, lean: 0.05),
]

NSGraphicsContext.saveGraphicsState()
let plantShadow = NSShadow()
plantShadow.shadowColor = color(0x053B33, 0.38)
plantShadow.shadowOffset = NSSize(width: 14, height: -10)
plantShadow.shadowBlurRadius = 28
plantShadow.set()
color(0xFFFFFF).setStroke()
stem.stroke()
color(0xFFFFFF).setFill()
leaves.forEach { $0.fill() }
NSGraphicsContext.restoreGraphicsState()

// Side shading gives the leaves volume; the stem stays flat white.
for (index, shape) in leaves.enumerated() {
  NSGraphicsContext.saveGraphicsState()
  shape.addClip()
  NSGradient(colors: [color(0xFFFFFF), color(0xE3FBEF), color(0xA6E8C8)],
    atLocations: [0, 0.5, 1], colorSpace: .sRGB)!.draw(in: shape.bounds, angle: index == 0 ? 160 : 135)
  NSGraphicsContext.restoreGraphicsState()
}

// Center veins.
color(0x1FA37A, 0.55).setStroke()
for (base, tip) in [(CGPoint(x: 516, y: 500), CGPoint(x: 345, y: 640)), (CGPoint(x: 520, y: 556), CGPoint(x: 715, y: 770))] {
  let vein = NSBezierPath()
  vein.move(to: base)
  vein.line(to: tip)
  vein.lineWidth = 8
  vein.lineCapStyle = .round
  vein.stroke()
}

// Development builds: an orange top-left corner reading "DEV" at 45°.
if developer {
  let leg: CGFloat = 400
  let corner = NSBezierPath()
  corner.move(to: CGPoint(x: 100, y: 924))
  corner.line(to: CGPoint(x: 100 + leg, y: 924))
  corner.line(to: CGPoint(x: 100, y: 924 - leg))
  corner.close()
  NSGraphicsContext.saveGraphicsState()
  let edge = NSShadow()
  edge.shadowColor = color(0x5A1A00, 0.35)
  edge.shadowOffset = NSSize(width: 6, height: -6)
  edge.shadowBlurRadius = 18
  edge.set()
  color(0xF26A1B).setFill()
  corner.fill()
  NSGraphicsContext.restoreGraphicsState()
  NSGradient(colors: [color(0xFFA24A), color(0xF26A1B), color(0xD9480F)],
    atLocations: [0, 0.6, 1], colorSpace: .sRGB)!.draw(in: corner, angle: -45)

  // Centered on the diagonal, between the rounded corner and the hypotenuse.
  let distance: CGFloat = 196
  context.saveGState()
  context.translateBy(x: 100 + distance / sqrt(2), y: 924 - distance / sqrt(2))
  context.rotate(by: .pi / 4)
  let label = NSAttributedString(string: "DEV", attributes: [
    .font: NSFont.systemFont(ofSize: 92, weight: .heavy),
    .foregroundColor: NSColor.white,
    .kern: 6,
  ])
  let bounds = label.size()
  label.draw(at: CGPoint(x: -bounds.width / 2, y: -bounds.height / 2))
  context.restoreGState()
}

// Thin inner highlight along the edge, as on Apple's own icons.
color(0xFFFFFF, 0.18).setStroke()
body.lineWidth = 4
body.stroke()
NSGraphicsContext.restoreGraphicsState()

NSGraphicsContext.current = nil
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output))
print("Wrote \(output)")
