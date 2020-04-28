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
    @EnvironmentObject var env : Env
    
    let context = CIContext()
    let filter = CIFilter.qrCodeGenerator()
    func generateQRCode(from string: String) -> UIImage {
        let data = Data(string.utf8)
        filter.setValue(data, forKey: "inputMessage")

        if let outputImage = filter.outputImage {
            if let cgimg = context.createCGImage(outputImage, from: outputImage.extent) {
                return UIImage(cgImage: cgimg)
            }
        }

        return UIImage()
    }
    var body: some View {
        VStack{
            Text("Server QR Code")
            Image(uiImage:generateQRCode(from: env.addr)).interpolation(.none)
            .resizable()
            .aspectRatio(contentMode: .fit)
        }
    }
}

struct QRCodeView_Previews: PreviewProvider {
    static var previews: some View {
        QRCodeView()
    }
}
