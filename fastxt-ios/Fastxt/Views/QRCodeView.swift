//
//  QRCodeView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/27/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI
import CoreImage.CIFilterBuiltins

struct QRCodeView: View {
    let text: String

    private let context = CIContext()
    private let filter = CIFilter.qrCodeGenerator()

    private func generate() -> UIImage {
        filter.setValue(Data(text.utf8), forKey: "inputMessage")
        if let outputImage = filter.outputImage,
           let cgimg = context.createCGImage(outputImage, from: outputImage.extent) {
            return UIImage(cgImage: cgimg)
        }
        return UIImage()
    }

    var body: some View {
        Image(uiImage: generate()).interpolation(.none)
            .resizable()
            .aspectRatio(contentMode: .fit)
            .frame(maxWidth: 200)
    }
}

struct QRCodeView_Previews: PreviewProvider {
    static var previews: some View {
        QRCodeView(text: "FASTXT1:192.168.1.1:3456:0123456789abcdef0123456789abcdef:ABCDEFGHIJ")
    }
}
