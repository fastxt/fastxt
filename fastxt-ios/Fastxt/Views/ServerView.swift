//
//  ServerView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/27/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI
import CoreImage.CIFilterBuiltins

struct ServerView: View {
    @EnvironmentObject var env : Env
    @State private var pairingCode: String = ""

    var body: some View {
        VStack(alignment: .leading){
            Text("As Server")
            HStack{
                if env.isServerRunning {
                    Button(action:{
                        DispatchQueue.background(background: {
                            _ = AppState.ft.run(json_input: #"{"action":"server-stop"}"#)
                        }, completion:{
                            DispatchQueue.main.async {
                                self.env.isServerRunning = false
                                self.pairingCode = ""
                            }
                        })
                    }){
                        Text("Stop Server")
                    }
                }else{
                    Button(action:{
                        DispatchQueue.background(background: {
                            let response = AppState.ft.run(json_input: #"{"action":"server-start"}"#)
                            var code = ""
                            if let data = response.data(using: .utf8),
                               let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                               let pairing = json["pairing_code"] as? String {
                                code = pairing
                            }
                            DispatchQueue.main.async {
                                if code.isEmpty {
                                    self.env.addr = "Could not start the server"
                                } else {
                                    self.pairingCode = code
                                    self.env.isServerRunning = true
                                }
                            }
                        })
                    }){
                        Text("Start Server")
                    }
                }
                Spacer()
            }

            if env.isServerRunning && !pairingCode.isEmpty {
                Text("Pairing code (scan on the other device):").font(.caption)
                Text(pairingCode).font(.system(.caption, design: .monospaced))
                    .contextMenu { Button(action:{ UIPasteboard.general.string = pairingCode }){ Text("Copy") } }
                QRCodeView(text: pairingCode)
            } else if !env.addr.isEmpty {
                Text(env.addr).font(.caption)
            }
            Spacer()
        }
        .onAppear {
            DispatchQueue.background(background: {
                let response = AppState.ft.run(json_input: #"{"action":"server-status"}"#)
                if let data = response.data(using: .utf8),
                   let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                   (json["running"] as? Bool) == true,
                   let code = json["pairing_code"] as? String {
                    DispatchQueue.main.async {
                        self.pairingCode = code
                        self.env.isServerRunning = true
                    }
                }
            })
        }
    }
}

struct ServerView_Previews: PreviewProvider {
    static var previews: some View {
        ServerView()
    }
}
