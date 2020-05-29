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
    @State private var syncStatus: String = ""
    var body: some View {
        VStack(alignment: .leading){
            Text("As Client")
            Text("Input other Fastxt server's address:port")
            TextField("xxx.xxx.xxx.xxx:3456", text: $addr)
            Button(action:{
                DispatchQueue.background(background: {
                    self.syncStatus = AppState.ft.run(json_input: """
                        {"action":"client-sync", "addr":"\(self.addr)"}
                    """)
                    let offset = AppState.getOffset()
                    AppState.search(input: AppState.getQuery(), offset: offset)
                }, completion:{
                    print("done client sync")
                })
            }){
                Text("Sync")
            }
            Text("Sync status:")
            Text(syncStatus)
            Spacer()
        }
    }
}

struct ClientView_Previews: PreviewProvider {
    static var previews: some View {
        ClientView()
    }
}
