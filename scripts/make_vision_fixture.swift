import AppKit
let image=NSImage(size:NSSize(width:1600,height:900))
image.lockFocus()
NSColor(calibratedWhite:0.12,alpha:1).setFill();NSRect(x:0,y:0,width:1600,height:900).fill()
NSColor(calibratedWhite:0.22,alpha:1).setFill();NSRect(x:100,y:250,width:650,height:560).fill()
func text(_ s:String,_ x:CGFloat,_ y:CGFloat,_ size:CGFloat){ (s as NSString).draw(at:NSPoint(x:x,y:y),withAttributes:[.font:NSFont.systemFont(ofSize:size),.foregroundColor:NSColor.white]) }
text("Calculadora",120,755,24);text("Padrão",120,705,32);text("12.323.232 + 4.343.434 =",290,640,24);text("16.666.666",280,560,64)
text("Histórico",850,750,24);text("123 + 456 = 579",850,690,28)
for (i,s) in ["7     8     9     ÷","4     5     6     ×","1     2     3     −","0     ,     =     +"].enumerated(){text(s,180,CGFloat(470-i*60),34)}
image.unlockFocus()
let rep=NSBitmapImageRep(data:image.tiffRepresentation!)!
try rep.representation(using:.png,properties:[:])!.write(to:URL(fileURLWithPath:CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "/tmp/agentsmith-ocr-fixture.png"))
