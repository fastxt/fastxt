//
//  ClientView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/27/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct ClientView: View {
    @State private var pairingCode: String = ""
    @State private var syncStatus: String = ""
    @State private var syncing: Bool = false
    @State private var foundResult: Bool = false

    var body: some View {
        VStack(alignment: .leading){
            Text("As Client")
            HStack{
                Button(action:{
                    self.foundResult = false
                    self.pairingCode = ""
                }){
                    Text("Rescan QR Code")
                }.disabled(!foundResult)
                Spacer()
                Button(action:{
                    self.syncing = true
                    self.syncStatus = "Syncing…"
                    let code = self.pairingCode
                    DispatchQueue.background(background: {
                        let escaped = code.replacingOccurrences(of: "\"", with: "\\\"")
                        let response = AppState.ft.run(json_input: """
                            {"action":"sync", "code":"\(escaped)"}
                        """)
                        var status = response
                        if let data = response.data(using: .utf8),
                           let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                            if let error = json["error"] as? String {
                                status = "✗ \(error)"
                            } else if let report = json["report"] as? [String: Any] {
                                status = """
                                    ✓ pulled \(report["notes_pulled"] ?? 0) notes, \
                                    pushed \(report["notes_pushed"] ?? 0), \
                                    embeddings +\(report["embeddings_pulled"] ?? 0)
                                    """
                            }
                        }
                        DispatchQueue.main.async {
                            self.syncStatus = status
                            self.syncing = false
                            let offset = AppState.getOffset()
                            AppState.search(input: AppState.getQuery(), offset: offset)
                        }
                    })
                }){
                     Text("Start Sync")
                }.disabled(pairingCode.isEmpty || syncing)
            }.padding()

            Text("Scan or paste the server's pairing code:")
            TextField("FASTXT1:…", text: $pairingCode).foregroundColor(.blue)
                .font(.system(.caption, design: .monospaced))

            Text("Sync status:")
            Text(syncStatus).font(.caption)

            if !foundResult {
                CodeScannerView(codeTypes: [.qr], simulatedData: "FASTXT1:192.168.1.1:3456:0123456789abcdef0123456789abcdef:ABCDEFGHIJ") { result in
                    switch result {
                    case .success(let code):
                        self.pairingCode = code
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
