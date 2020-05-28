//
//  ClientView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/27/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct ClientView: View {
    @State private var scanResult: String = ""
    @State private var syncStatus: String = ""
    @State private var foundResult: Bool = false
    var body: some View {
        VStack(alignment: .leading){
            Text("As Client")
            HStack{
                Button(action:{
                    self.foundResult = false
                    self.scanResult = ""
                }){
                    Text("Rescan QR Code")
                }.disabled(!foundResult)
                Spacer()
                Button(action:{
                    self.syncStatus = AppState.ft.run(json_input: """
                        {"action":"client-sync",
                        "addr":"\(self.scanResult)"}
                        """)
                    let offset = AppState.getOffset()
                    AppState.search(input: AppState.getQuery(), offset: offset)
                }){
                     Text("Start Sync")
                }
            }.padding()
                
            Text("Scan or input server address:port below:")
            TextField("xxx.xxx.xxx.xxx:YYYYY ", text: $scanResult).foregroundColor(.blue)
        
            Text("Sync status:")
            Text(syncStatus)
            
            if !foundResult {
                CodeScannerView(codeTypes: [.qr], simulatedData: "0.0.0.0:3456") { result in
                    switch result {
                    case .success(let code):
                        print("Found code: \(code)")
                        self.scanResult = code
                        self.foundResult = true
                    case .failure(let error):
                        self.foundResult = false
                        print(error.localizedDescription)
                    }
                }
            }
            Spacer()

        }.padding()
    }
}

struct ClientView_Previews: PreviewProvider {
    static var previews: some View {
        ClientView()
    }
}
