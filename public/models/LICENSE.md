# Models

`u2netp.onnx` — **U²-Netp**, the small (4.6 MB) variant of U²-Net, a salient
object detection network by Xuebin Qin et al.
([U-2-Net](https://github.com/xuebinqin/U-2-Net)), licensed under the
[Apache License 2.0](u2netp.LICENSE.txt). The ONNX export is the one
distributed by the [rembg](https://github.com/danielgatis/rembg) project
(release `v0.0.0`, SHA-256
`309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8`).

Tessera runs it in the window/browser with
[onnxruntime-web](https://github.com/microsoft/onnxruntime) (MIT) to cut the
subject out of a photo; nothing leaves the machine.
