//
//  ClientView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/25/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct ClientView: View {
    @State private var addr:String = ""
    var body: some View {
        VStack(alignment: .leading){
            Text("As Client")
            Text("Input other Fastxt server's address:port")
            TextField("xxx.xxx.xxx.xxx:3456", text: $addr)
            Button(action:{
                DispatchQueue.background(background: {
                    AppState.ft.run(json_input: """
                        {"action":"client-sync", "addr":"\(self.addr)"}
                    """)
                }, completion:{
                    print("done client sync")
                })
            }){
                Text("Sync")
            }
            Spacer()
        }
    }
}

struct ClientView_Previews: PreviewProvider {
    static var previews: some View {
        ClientView()
    }
}
